import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:cryptography/cryptography.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:unifiedpush_platform_interface/unifiedpush_platform_interface.dart';
import 'package:unifiedpush_platform_interface/data/push_endpoint.dart';
import 'package:unifiedpush_platform_interface/data/push_message.dart';

import '../model/core_config.dart';
import '../state/app_state.dart';
import 'notification_service.dart';
import 'relay_capabilities.dart';

/// Native Android connector and APNs bridge. Push carries a wakeup, never chat data.
class MobilePushService {
  MobilePushService._(
      {bool? android,
      bool? ios,
      FlutterSecureStorage? storage,
      MethodChannel? channel})
      : _androidOverride = android,
        _iosOverride = ios,
        _storage = storage ??
            const FlutterSecureStorage(
                iOptions: IOSOptions(
                    accessibility:
                        KeychainAccessibility.first_unlock_this_device)),
        _channel = channel ?? const MethodChannel('org.fedi3.client/push');
  @visibleForTesting
  factory MobilePushService.forTesting(
          {required bool android,
          required bool ios,
          required FlutterSecureStorage storage,
          required MethodChannel channel}) =>
      MobilePushService._(
          android: android, ios: ios, storage: storage, channel: channel);
  final bool? _androidOverride;
  final bool? _iosOverride;
  bool get _android => _androidOverride ?? Platform.isAndroid;
  bool get _ios => _iosOverride ?? Platform.isIOS;
  static final instance = MobilePushService._();
  static bool get supported => Platform.isAndroid || Platform.isIOS;
  final MethodChannel _channel;
  final FlutterSecureStorage _storage;
  static const _settingsKey = 'fedi3.mobilePush.v1';
  final status = ValueNotifier<String>('');
  AppState? _app;
  CoreConfig? _boundConfig;
  String _scope = '';
  bool _background = false;
  bool _headless = false;
  bool enabled = false;
  bool _initialized = false;
  Future<bool>? _sync;
  int _generation = 0;
  Future<void> _settingsWrites = Future<void>.value();
  Map<String, dynamic>? _registration;
  String? _vapid;

  Future<void> initialize(AppState app, {bool background = false}) async {
    if ((!_android && !_ios) || _initialized) return;
    _initialized = true;
    _app = app;
    _background = background;
    _headless = background;
    _boundConfig = app.config;
    if (app.config != null) _scope = await scopeFor(app.config!);
    final raw = await _storage.read(key: _settingsKey);
    try {
      final settings =
          raw == null ? null : jsonDecode(raw) as Map<String, dynamic>;
      enabled = settings?['enabled'] == true && settings?['scope'] == _scope;
      _vapid = settings?['vapid'] as String?;
      _registration = settings?['scope'] == _scope
          ? (settings?['registration'] as Map<String, dynamic>?)
          : null;
    } catch (_) {
      enabled = false;
    }
    app.addListener(_accountChanged);
    if (_android) {
      await UnifiedPushPlatform.instance.initializeCallback(
        onNewEndpoint: (endpoint, scope) {
          unawaited(_newEndpoint(endpoint, scope));
        },
        onMessage: (message, scope) {
          unawaited(_message(message, scope));
        },
        onUnregistered: (scope) {
          if (enabled && scope == _scope) {
            status.value = 'unregistered';
            unawaited(_removeRelayRegistration());
          }
        },
        onRegistrationFailed: (reason, scope) {
          if (scope == _scope) status.value = 'registration_${reason.name}';
        },
      );
      await UnifiedPushPlatform.instance.initializeOnTempUnavailable((scope) {
        if (scope == _scope) status.value = 'temporarily_unavailable';
      });
      if (enabled && !background) await _registerAndroid();
    } else {
      _channel.setMethodCallHandler((call) async {
        if (call.method == 'token' && call.arguments is String && enabled) {
          _registration = {'provider': 'apns', 'token': call.arguments};
          await _persist();
          await _registerRelay();
        } else if (call.method == 'registrationFailed') {
          status.value = 'apns_registration_failed';
        } else if (call.method == 'wake') {
          bool updated = false;
          try {
            if (enabled) updated = await wake();
          } finally {
            await _channel.invokeMethod<void>(
                'completeWake', {'id': call.arguments, 'updated': updated});
          }
        }
      });
      _background = background ||
          (await _channel.invokeMethod<bool>('initialize') ?? false);
      if (enabled && !background) await _channel.invokeMethod<void>('register');
    }
    if (enabled && !background) unawaited(_registerRelay());
  }

  @visibleForTesting
  static Future<String> scopeFor(CoreConfig config) async =>
      base64UrlEncode((await Sha256().hash(utf8.encode(
                  '${Uri.parse(config.relayWs).replace(scheme: Uri.parse(config.relayWs).scheme == 'wss' ? 'https' : 'http').origin}\n${config.username}')))
              .bytes)
          .replaceAll('=', '');

  void _accountChanged() {
    final current = _app?.config;
    if (_boundConfig == null ||
        current == null ||
        current.username != _boundConfig!.username ||
        current.relayWs != _boundConfig!.relayWs) {
      if (enabled) unawaited(disable());
    }
  }

  void setForeground(bool foreground) {
    if (!_android && !_ios) return;
    _background = !foreground;
    if (foreground && enabled) unawaited(_registerRelay());
  }

  Future<List<String>> distributors() async => _android
      ? await UnifiedPushPlatform.instance.getDistributors(const [])
      : [];

  Future<void> enable({String? distributor}) async {
    final generation = ++_generation;
    final cfg = _app?.config;
    if (cfg == null) throw StateError('Account required');
    final config = await RelayCapabilities.pushConfig(cfg);
    if (generation != _generation) return;
    final provider = _android ? 'unifiedpush' : 'apns';
    if (config['enabled'] != true ||
        !(config['providers'] as List).contains(provider)) {
      throw StateError('Push provider not configured on relay');
    }
    // Re-save existing iOS credentials under the background-accessible,
    // device-only keychain class when the user explicitly enables push.
    if (_ios) await _app!.saveConfig(cfg);
    final nextScope = await scopeFor(cfg);
    if (nextScope != _scope) _registration = null;
    _boundConfig = cfg;
    _scope = nextScope;
    _vapid = config['vapid_public_key'] as String?;
    enabled = true;
    await _persist();
    if (generation != _generation) return;
    try {
      if (_android) {
        if (distributor != null) {
          await UnifiedPushPlatform.instance.saveDistributor(distributor);
        }
        if (await UnifiedPushPlatform.instance.getDistributor() == null) {
          if (!await UnifiedPushPlatform.instance
              .tryUseCurrentOrDefaultDistributor()) {
            throw StateError('Choose a UnifiedPush distributor');
          }
        }
        await NotificationService.requestPermission();
        await _registerAndroid();
      } else {
        await _channel.invokeMethod<void>('enable');
      }
      status.value = 'registering';
    } catch (_) {
      if (generation == _generation) {
        enabled = false;
        await _persist();
      }
      rethrow;
    }
  }

  Future<void> _registerAndroid() =>
      UnifiedPushPlatform.instance.register(_scope, const [], 'Fedi3', _vapid);

  Future<void> _newEndpoint(PushEndpoint endpoint, String scope) async {
    if (!enabled || scope != _scope || endpoint.pubKeySet == null) return;
    _registration = {
      'provider': 'unifiedpush',
      'token': endpoint.url,
      'webpush_keys': {
        'pub_key': endpoint.pubKeySet!.pubKey,
        'auth': endpoint.pubKeySet!.auth
      }
    };
    await _persist();
    await _registerRelay();
  }

  Future<void> _registerRelay() async {
    final cfg = _app?.config;
    final registration = _registration;
    if (!enabled ||
        cfg == null ||
        registration == null ||
        await scopeFor(cfg) != _scope) {
      return;
    }
    try {
      await RelayCapabilities.registerPush(cfg,
          registration['provider'] as String, registration['token'] as String,
          webpushKeys:
              (registration['webpush_keys'] as Map?)?.cast<String, String>());
      status.value = 'registered';
    } catch (_) {
      status.value = 'relay_registration_pending';
    }
  }

  Future<void> _removeRelayRegistration([CoreConfig? captured]) async {
    final cfg = captured ?? _boundConfig;
    if (cfg != null) {
      try {
        await RelayCapabilities.unregisterPush(cfg);
      } catch (_) {
        status.value = 'revocation_pending';
      }
    }
  }

  Future<void> disable() async {
    final generation = ++_generation;
    final config = _boundConfig;
    final scope = _scope;
    enabled = false;
    await _persist();
    await _removeRelayRegistration(config);
    if (_android && scope.isNotEmpty) {
      await UnifiedPushPlatform.instance.unregister(scope);
    }
    if (_ios && generation == _generation) {
      await _channel.invokeMethod<void>('disable');
    }
    if (generation == _generation) {
      _registration = null;
      await _persist();
      if (status.value != 'revocation_pending') status.value = 'disabled';
    }
  }

  Future<void> _persist() {
    final value = jsonEncode({
      'enabled': enabled,
      'scope': _scope,
      'vapid': _vapid,
      'registration': _registration
    });
    final write = _settingsWrites
        .then((_) => _storage.write(key: _settingsKey, value: value));
    _settingsWrites =
        write.then<void>((_) {}, onError: (Object _, StackTrace __) {});
    return write;
  }

  @visibleForTesting
  static bool validWake(List<int> content, {required bool decrypted}) {
    if (!decrypted || content.isEmpty || content.length > 4096) return false;
    try {
      final value = jsonDecode(utf8.decode(content));
      return value is Map && value['event'] == 'sync';
    } catch (_) {
      return false;
    }
  }

  Future<void> _message(PushMessage message, String scope) async {
    if (enabled &&
        scope == _scope &&
        validWake(message.content, decrypted: message.decrypted)) {
      await wake();
    }
  }

  /// Coalesce wakeups; never keep a core alive indefinitely in the background.
  Future<bool> wake() => _sync ??= _wake().whenComplete(() => _sync = null);
  Future<bool> _wake() async {
    final app = _app;
    final cfg = app?.config;
    if (!enabled ||
        app == null ||
        cfg == null ||
        await scopeFor(cfg) != _scope) {
      return false;
    }
    if (_android && _headless) {
      // A second Flutter engine must not compete with the UI for the native core.
      // Android wakes notify; opening/resuming the UI drains the durable spool.
      await NotificationService.showChatNotification(
          title: 'Fedi3',
          body: app.prefs.localeTag == 'it'
              ? 'Nuove attività disponibili'
              : 'New activity available');
      return false;
    }
    final started = !app.isRunning;
    bool hadItems = false;
    try {
      final pending = await RelayCapabilities.pendingPushItems(cfg);
      hadItems = pending > 0;
      if (pending == 0) return false;
      if (started) await app.startCore();
      final deadline = DateTime.now().add(const Duration(seconds: 10));
      while (DateTime.now().isBefore(deadline)) {
        final count = await RelayCapabilities.pendingPushItems(cfg);
        hadItems = hadItems || count > 0;
        if (count == 0) break;
        await Future<void>.delayed(const Duration(milliseconds: 400));
      }
      if (_background && hadItems) {
        await NotificationService.showChatNotification(
            title: 'Fedi3',
            body: app.prefs.localeTag == 'it'
                ? 'Nuove attività disponibili'
                : 'New activity available');
      }
      return hadItems;
    } catch (_) {
      status.value = 'sync_deferred';
      return false;
    } finally {
      if (started && _background) {
        try {
          await app.stopCore().timeout(const Duration(seconds: 5));
        } catch (_) {}
      }
    }
  }
}
