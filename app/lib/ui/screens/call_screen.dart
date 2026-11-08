/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:convert';
import 'dart:math';

import 'package:flutter/foundation.dart'
    show defaultTargetPlatform, TargetPlatform;
import 'package:flutter/material.dart';
import 'package:flutter_webrtc/flutter_webrtc.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/chat_models.dart';
import '../../services/core_event_stream.dart';
import '../../services/call_ice_config.dart';
import '../../services/relay_capabilities.dart';
import '../../services/screen_share_service.dart';
import '../../state/app_state.dart';
import '../theme/ui_tokens.dart';

/// WebRTC audio/video: ICE tries direct paths and configured TURN operators.
/// Chat carries encrypted signaling; group calls use a mesh of peer connections.
class CallScreen extends StatefulWidget {
  const CallScreen({
    super.key,
    required this.appState,
    required this.threadId,
    required this.title,
    required this.mediaType,
    this.isGroup = false,
    this.incoming = false,
    this.callId,
    this.remoteActor,
  });

  final AppState appState;
  final String threadId;
  final String title;
  final String mediaType; // 'audio' or 'video'
  final bool isGroup;
  final bool incoming;
  final String? callId;
  final String? remoteActor;

  /// Number of call screens currently mounted. The shell uses it to keep
  /// transient banners (announcements, new messages) off the call controls.
  static int active = 0;

  @override
  State<CallScreen> createState() => _CallScreenState();
}

class _CallScreenState extends State<CallScreen> {
  late final String _callId;
  final Map<String, RTCPeerConnection> _pcs = {};
  final Map<String, RTCVideoRenderer> _remoteRenderers = {};
  final Map<String, List<RTCIceCandidate>> _pendingIce = {};
  final Set<String> _participants = {};
  final Set<String> _connected = {};
  final Set<String> _rosterDone = {};
  MediaStream? _localStream;
  MediaStream? _screenStream;
  RTCVideoRenderer? _localRenderer;
  final Map<String, RTCRtpSender> _videoSenders = {};
  final Map<String, RTCRtpSender> _audioSenders = {};
  final Map<String, RTCRtpSender> _addedScreenVideo = {};
  final Map<String, RTCRtpSender> _addedScreenAudio = {};
  MediaStreamTrack? _cameraTrack;
  MediaStreamTrack? _micTrack;
  bool _micOn = true;
  bool _cameraOn = true;
  bool _screenSharing = false;
  bool _ended = false;
  bool _connecting = true;
  String? _error;
  int _lastSignalMs = 0;
  int _lastSignalId = 0;
  Timer? _signalTimer;
  Timer? _retrySignal;
  Map<String, dynamic>? _retryData;
  int _retryCount = 0;
  StreamSubscription<CoreEvent>? _streamSub;
  Timer? _durationTimer;
  Timer? _mediaStatsTimer;
  bool _collectingMediaStats = false;
  DateTime? _startedAt;
  bool _fetching = false;

  String get _selfActor {
    final cfg = widget.appState.config;
    if (cfg == null) return '';
    return '${cfg.publicBaseUrl.replaceAll(RegExp(r'/+$'), '')}/users/${cfg.username}';
  }

  @override
  void initState() {
    super.initState();
    CallScreen.active++;
    // A banner already on screen would cover the call controls.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) ScaffoldMessenger.of(context).hideCurrentSnackBar();
    });
    _callId = widget.callId?.trim().isNotEmpty == true
        ? widget.callId!.trim()
        : 'call-${DateTime.now().microsecondsSinceEpoch}-${Random().nextInt(9999)}';
    _lastSignalMs = DateTime.now().millisecondsSinceEpoch;
    _start();
  }

  @override
  void dispose() {
    _ended = true;
    CallScreen.active = (CallScreen.active - 1).clamp(0, 1 << 30);
    _signalTimer?.cancel();
    _retrySignal?.cancel();
    _streamSub?.cancel();
    _durationTimer?.cancel();
    _mediaStatsTimer?.cancel();
    for (final r in _remoteRenderers.values) {
      r.dispose();
    }
    _localRenderer?.dispose();
    for (final pc in _pcs.values) {
      pc.close();
    }
    final stream = _localStream;
    for (final track in [
      ...?_localStream?.getTracks(),
      ...?_screenStream?.getTracks(),
    ]) {
      unawaited(track.stop());
    }
    if (stream != null) unawaited(stream.dispose());
    _localStream = null;
    _screenStream = null;
    unawaited(Helper.setSpeakerphoneOn(false));
    super.dispose();
  }

  Future<void> _start() async {
    try {
      final renderer = RTCVideoRenderer();
      await renderer.initialize();
      if (_ended || !mounted) {
        await renderer.dispose();
        return;
      }
      _localRenderer = renderer;
      final stream = await navigator.mediaDevices.getUserMedia({
        'audio': true,
        'video': widget.mediaType == 'video'
            ? {
                'facingMode': 'user',
                'width': 1280,
                'height': 720,
                'frameRate': 30
              }
            : false,
      });
      if (_ended || !mounted) {
        for (final track in stream.getTracks()) {
          await track.stop();
        }
        await stream.dispose();
        return;
      }
      _localStream = stream;
      _micTrack = _localStream!.getAudioTracks().isNotEmpty
          ? _localStream!.getAudioTracks().first
          : null;
      _cameraTrack = _localStream!.getVideoTracks().isNotEmpty
          ? _localStream!.getVideoTracks().first
          : null;
      if (_localRenderer != null && widget.mediaType == 'video') {
        _localRenderer!.srcObject = _localStream;
      }
      if (widget.incoming && widget.remoteActor != null) {
        _participants.add(widget.remoteActor!);
        await _signal('accept');
        // A lost accept leaves both peers waiting forever: keep re-announcing
        // it until the caller's offer arrives.
        _startSignalRetry('accept');
      } else {
        final inviteData = {
          'media': widget.mediaType,
          'title': widget.title,
        };
        await _signal('invite', data: inviteData);
        // Re-send the invite while ringing. The receiver deduplicates identical
        // invites, but a copy that was dropped as stale is stored on retry.
        _startSignalRetry('invite', data: inviteData);
      }
      if (_ended || !mounted) return;
      _durationTimer = Timer.periodic(const Duration(seconds: 1), (_) {
        if (mounted) setState(() {});
      });
      _signalTimer = Timer.periodic(const Duration(milliseconds: 400), (_) {
        _fetchSignals();
      });
      _streamSub = CoreEventStream(config: widget.appState.config!)
          .stream(kind: 'chat')
          .listen((ev) {
        if (ev.kind != 'chat') return;
        if ((ev.activityId ?? '') != widget.threadId) return;
        if (ev.activityType == 'call') _fetchSignals();
      }, onError: (_) {}, onDone: () {});
      _fetchSignals();
      if (mounted) setState(() => _connecting = true);
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  void _startSignalRetry(String action, {Map<String, dynamic>? data}) {
    _retrySignal?.cancel();
    _retryData = data;
    _retryCount = 0;
    _retrySignal = Timer.periodic(const Duration(seconds: 3), (_) async {
      if (_ended || !mounted) {
        _retrySignal?.cancel();
        return;
      }
      _retryCount++;
      // 20 attempts cover a full minute of ringing without flooding the peer.
      if (_retryCount > 20) {
        _retrySignal?.cancel();
        // Nobody answered within a minute: leave so the peer can stop ringing.
        unawaited(_end());
        return;
      }
      await _signal(action, data: _retryData);
    });
  }

  void _cancelSignalRetry() {
    _retrySignal?.cancel();
    _retrySignal = null;
    _retryData = null;
    _retryCount = 0;
  }

  Future<void> _signal(
    String action, {
    String? to,
    Map<String, dynamic>? data,
  }) async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    try {
      await CoreApi(config: cfg).sendChatCallSignal(
        threadId: widget.threadId,
        callId: _callId,
        action: action,
        to: to,
        data: data,
      );
      // A later retry can repair a transient delivery failure.
      if (mounted && !_ended && _error != null) setState(() => _error = null);
    } catch (e) {
      if (mounted && !_ended) setState(() => _error = e.toString());
    }
  }

  Future<void> _fetchSignals() async {
    final cfg = widget.appState.config;
    if (cfg == null || _fetching || _ended) return;
    _fetching = true;
    try {
      final items = await CoreApi(config: cfg).fetchChatCallSignals(
        threadId: widget.threadId,
        afterMs: _lastSignalMs,
        afterId: _lastSignalId,
      );
      for (final s in items) {
        if (s.id > _lastSignalId) _lastSignalId = s.id;
        if (s.callId != _callId) continue;
        if (s.createdAtMs > _lastSignalMs) _lastSignalMs = s.createdAtMs;
        await _handleSignal(s);
      }
    } catch (_) {
    } finally {
      _fetching = false;
    }
  }

  Future<void> _handleSignal(ChatCallSignal s) async {
    final from = s.fromActor.trim();
    if (from.isEmpty || from == _selfActor) return;
    switch (s.action) {
      case 'invite':
        break;
      case 'accept':
        _cancelSignalRetry();
        final newlyAccepted = _participants.add(from);
        if (!widget.incoming && newlyAccepted) {
          // The caller creates the first offer. A two-person roster is empty,
          // so waiting for roster processing leaves both peers idle forever.
          await _createOffer(from);
          final roster = _participants.where((a) => a != from).toList();
          await _signal('roster', to: from, data: {
            'participants': roster,
            'media': widget.mediaType,
          });
        }
        break;
      case 'roster':
        _cancelSignalRetry();
        final raw = s.data['participants'];
        final list = raw is List
            ? raw.map((e) => e.toString()).where((e) => e.isNotEmpty).toList()
            : <String>[];
        for (final actor in list) {
          if (actor == _selfActor || _rosterDone.contains(actor)) continue;
          _rosterDone.add(actor);
          _participants.add(actor);
          await _createOffer(actor);
        }
        break;
      case 'offer':
        _cancelSignalRetry();
        await _handleOffer(s, from);
        break;
      case 'answer':
        final pc = _pcs[from];
        final sdp = s.data['sdp']?.toString();
        if (pc != null && sdp != null) {
          await pc.setRemoteDescription(
            RTCSessionDescription(sdp, s.data['type']?.toString() ?? 'answer'),
          );
          await _flushIce(from);
          if (mounted) setState(() {});
        }
        break;
      case 'ice':
        final pc = _pcs[from];
        final cand = s.data['candidate']?.toString();
        if (cand == null) return;
        final ice = RTCIceCandidate(
          cand,
          s.data['sdp_mid']?.toString(),
          (s.data['sdp_mline_index'] as num?)?.toInt(),
        );
        if (pc == null || await pc.getRemoteDescription() == null) {
          _pendingIce.putIfAbsent(from, () => []).add(ice);
        } else {
          await pc.addCandidate(ice);
        }
        break;
      case 'decline':
        _cancelSignalRetry();
        _participants.remove(from);
        _connected.remove(from);
        await _closePeer(from);
        if (mounted) setState(() {});
        if (!widget.isGroup && _participants.isEmpty) _end(notify: false);
        break;
      case 'leave':
        _cancelSignalRetry();
        _participants.remove(from);
        _connected.remove(from);
        await _closePeer(from);
        if (mounted) setState(() {});
        if (_participants.isEmpty) _end(notify: false);
        break;
      case 'screen':
        if (mounted) setState(() {});
        break;
    }
  }

  Future<List<Map<String, dynamic>>>? _communityIce;

  Future<RTCPeerConnection> _peer(String actor) async {
    final existing = _pcs[actor];
    if (existing != null) return existing;
    final cfg = widget.appState.config;
    final iceUrls = cfg?.webrtcIceUrls ?? const <String>[];
    final servers = [
      ...buildCallIceServers(
        iceUrls,
        username: cfg?.webrtcIceUsername,
        credential: cfg?.webrtcIceCredential,
      )
    ];
    if (cfg != null) {
      _communityIce ??= RelayCapabilities.turnPool(cfg);
      servers.addAll(await _communityIce!);
    }
    if (_ended || !mounted) throw StateError('call ended during ICE setup');
    final pc = await createPeerConnection({
      'iceServers': servers,
      'sdpSemantics': 'unified-plan',
    });
    if (_ended || !mounted) {
      await pc.close();
      throw StateError('call ended during connection setup');
    }
    _pcs[actor] = pc;
    _mediaStatsTimer ??= Timer.periodic(const Duration(seconds: 5), (_) {
      _logMediaStats();
    });
    pc.onIceCandidate = (candidate) {
      if (candidate.candidate == null) return;
      _signal('ice', to: actor, data: {
        'candidate': candidate.candidate,
        'sdp_mid': candidate.sdpMid,
        'sdp_mline_index': candidate.sdpMLineIndex,
      });
    };
    pc.onTrack = (event) {
      if (event.streams.isEmpty) return;
      final stream = event.streams.first;
      if (event.track.kind == 'video') {
        _ensureRenderer(actor, stream);
      }
    };
    pc.onConnectionState = (state) {
      if (state == RTCPeerConnectionState.RTCPeerConnectionStateConnected) {
        _connected.add(actor);
        _connecting = false;
        _error = null;
        _startedAt ??= DateTime.now();
        _mediaStatsTimer ??= Timer.periodic(const Duration(seconds: 5), (_) {
          _logMediaStats();
        });
      } else if (state == RTCPeerConnectionState.RTCPeerConnectionStateFailed ||
          state == RTCPeerConnectionState.RTCPeerConnectionStateClosed) {
        _connected.remove(actor);
        _connecting = false;
        if (mounted &&
            state == RTCPeerConnectionState.RTCPeerConnectionStateFailed) {
          _error = context.l10n.networkErrorTitle;
        }
      }
      if (mounted) setState(() {});
    };
    if (_localStream != null) {
      for (final track in _localStream!.getTracks()) {
        final sender = await pc.addTrack(track, _localStream!);
        if (track.kind == 'video') _videoSenders[actor] = sender;
        if (track.kind == 'audio') _audioSenders[actor] = sender;
      }
    }
    return pc;
  }

  Future<void> _logMediaStats() async {
    if (_collectingMediaStats || _ended) return;
    _collectingMediaStats = true;
    try {
      for (final entry in _pcs.entries.toList()) {
        final reports = await entry.value.getStats();
        var sent = 0;
        var received = 0;
        for (final report in reports) {
          if (report.type == 'outbound-rtp') {
            sent += (report.values['bytesSent'] as num?)?.toInt() ?? 0;
          } else if (report.type == 'inbound-rtp') {
            received += (report.values['bytesReceived'] as num?)?.toInt() ?? 0;
          }
        }
        debugPrint('Fedi3 call media: ${jsonEncode({
              'call_id': _callId,
              'peer': _actorName(entry.key),
              'connected': _connected.contains(entry.key),
              'connection_state': entry.value.connectionState?.name,
              'ice_state': entry.value.iceConnectionState?.name,
              'candidate_pairs': reports
                  .where((r) => r.type == 'candidate-pair')
                  .take(8)
                  .map((r) => {
                        'state': r.values['state'],
                        'nominated': r.values['nominated'],
                        'requests_sent': r.values['requestsSent'],
                        'responses_received': r.values['responsesReceived'],
                      })
                  .toList(),
              'rtp_bytes_sent': sent,
              'rtp_bytes_received': received,
            })}');
      }
    } catch (_) {
      // A peer may close while its final statistics are being collected.
    } finally {
      _collectingMediaStats = false;
    }
  }

  Future<void> _ensureRenderer(String actor, MediaStream stream) async {
    var renderer = _remoteRenderers[actor];
    if (renderer == null) {
      renderer = RTCVideoRenderer();
      await renderer.initialize();
      if (_ended || !mounted) {
        await renderer.dispose();
        return;
      }
      _remoteRenderers[actor] = renderer;
    }
    renderer.srcObject = stream;
    if (mounted) setState(() {});
  }

  Future<void> _createOffer(String actor) async {
    final pc = await _peer(actor);
    final offer = await pc.createOffer();
    await pc.setLocalDescription(offer);
    await _signal('offer', to: actor, data: {
      'sdp': offer.sdp,
      'type': offer.type,
    });
  }

  Future<void> _handleOffer(ChatCallSignal s, String from) async {
    final pc = await _peer(from);
    final sdp = s.data['sdp']?.toString();
    if (sdp == null) return;
    await pc.setRemoteDescription(
      RTCSessionDescription(sdp, s.data['type']?.toString() ?? 'offer'),
    );
    await _flushIce(from);
    final answer = await pc.createAnswer();
    await pc.setLocalDescription(answer);
    await _signal('answer', to: from, data: {
      'sdp': answer.sdp,
      'type': answer.type,
    });
    _participants.add(from);
    if (mounted) setState(() {});
  }

  Future<void> _flushIce(String actor) async {
    final queued = _pendingIce.remove(actor);
    if (queued == null) return;
    final pc = _pcs[actor];
    if (pc == null) return;
    for (final c in queued) {
      await pc.addCandidate(c);
    }
  }

  Future<void> _closePeer(String actor) async {
    final pc = _pcs.remove(actor);
    if (pc != null) {
      // Detach the senders before closing: an audio sender still attached to a
      // live connection keeps WebRTC's audio device recording after the call,
      // which costs CPU and holds the microphone.
      final senders = <RTCRtpSender>[];
      for (final map in [
        _audioSenders,
        _videoSenders,
        _addedScreenAudio,
        _addedScreenVideo,
      ]) {
        final sender = map.remove(actor);
        if (sender != null) senders.add(sender);
      }
      for (final sender in senders) {
        try {
          await pc.removeTrack(sender);
        } catch (_) {
          // A sender can already be gone when the connection failed early.
        }
      }
      await pc.close();
      // Dispose releases the native peer connection; without it the shared
      // audio device keeps its playout thread running after the call.
      await pc.dispose();
    }
    final renderer = _remoteRenderers.remove(actor);
    renderer?.dispose();
  }

  void _toggleMic() {
    _micOn = !_micOn;
    _micTrack?.enabled = _micOn;
    setState(() {});
  }

  void _toggleCamera() {
    _cameraOn = !_cameraOn;
    _cameraTrack?.enabled = _cameraOn;
    setState(() {});
  }

  Future<void> _toggleScreenShare() async {
    if (_screenSharing) {
      await _stopScreenShare();
    } else {
      await _startScreenShare();
    }
  }

  Future<void> _startScreenShare() async {
    try {
      // Android needs the mediaProjection foreground service before the
      // projection is created, otherwise getDisplayMedia kills the process.
      await ScreenShareService.start();
      final stream = await navigator.mediaDevices.getDisplayMedia({
        'video': true,
        'audio': true,
      });
      if (_ended || !mounted) {
        for (final track in stream.getTracks()) {
          await track.stop();
        }
        await stream.dispose();
        return;
      }
      _screenStream = stream;
      final video = stream.getVideoTracks().isNotEmpty
          ? stream.getVideoTracks().first
          : null;
      final audio = stream.getAudioTracks().isNotEmpty
          ? stream.getAudioTracks().first
          : null;
      final added = <String>[];
      for (final entry in _pcs.entries) {
        final actor = entry.key;
        final pc = entry.value;
        final videoSender = _videoSenders[actor];
        final audioSender = _audioSenders[actor];
        if (video != null) {
          if (videoSender != null) {
            await videoSender.replaceTrack(video);
          } else {
            _addedScreenVideo[actor] = await pc.addTrack(video, stream);
            added.add(actor);
          }
        }
        if (audio != null) {
          if (audioSender != null) {
            await audioSender.replaceTrack(audio);
          } else {
            _addedScreenAudio[actor] = await pc.addTrack(audio, stream);
            if (!added.contains(actor)) added.add(actor);
          }
        }
      }
      for (final actor in added) {
        await _createOffer(actor);
      }
      _screenSharing = true;
      if (_localRenderer != null) _localRenderer!.srcObject = stream;
      await _signal('screen', data: {'enabled': true});
      if (mounted) setState(() {});
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  Future<void> _stopScreenShare() async {
    final stream = _screenStream;
    _screenStream = null;
    final renegotiate = <String>[];
    for (final entry in _pcs.entries) {
      final actor = entry.key;
      final pc = entry.value;
      final addedVideo = _addedScreenVideo.remove(actor);
      final addedAudio = _addedScreenAudio.remove(actor);
      final videoSender = _videoSenders[actor];
      final audioSender = _audioSenders[actor];
      if (_cameraTrack != null && videoSender != null) {
        await videoSender.replaceTrack(_cameraTrack);
      }
      if (_micTrack != null && audioSender != null) {
        await audioSender.replaceTrack(_micTrack);
      }
      if (addedVideo != null) {
        await pc.removeTrack(addedVideo);
        renegotiate.add(actor);
      }
      if (addedAudio != null) {
        await pc.removeTrack(addedAudio);
        if (!renegotiate.contains(actor)) renegotiate.add(actor);
      }
    }
    for (final actor in renegotiate) {
      await _createOffer(actor);
    }
    stream?.getTracks().forEach((t) => t.stop());
    await ScreenShareService.stop();
    _screenSharing = false;
    if (_localRenderer != null && _localStream != null) {
      _localRenderer!.srcObject = _localStream;
    }
    await _signal('screen', data: {'enabled': false});
    if (mounted) setState(() {});
  }

  Future<void> _end({bool notify = true}) async {
    if (_ended) return;
    _ended = true;
    _signalTimer?.cancel();
    _retrySignal?.cancel();
    _durationTimer?.cancel();
    _mediaStatsTimer?.cancel();
    // Stop capture immediately, even when signaling is offline or queued.
    if (notify) unawaited(_signal('leave'));
    final peers = _pcs.keys.toList();
    _localStream?.getTracks().forEach((t) => t.stop());
    _screenStream?.getTracks().forEach((t) => t.stop());
    if (mounted) Navigator.of(context).pop();
    // Closing a peer connection can block when it never connected, so the
    // screen must not wait for it.
    for (final actor in peers) {
      unawaited(_closePeer(actor)
          .timeout(const Duration(seconds: 3), onTimeout: () {}));
    }
  }

  String _actorName(String actor) {
    final uri = Uri.tryParse(actor);
    if (uri == null) return actor;
    for (final seg in uri.pathSegments.reversed) {
      if (seg.isNotEmpty) return seg;
    }
    return actor;
  }

  String _durationLabel() {
    final start = _startedAt;
    if (start == null) return '00:00';
    final d = DateTime.now().difference(start);
    final m = d.inMinutes.toString().padLeft(2, '0');
    final s = (d.inSeconds % 60).toString().padLeft(2, '0');
    return '$m:$s';
  }

  Widget _remoteTile(String actor) {
    final renderer = _remoteRenderers[actor];
    return Container(
      decoration: BoxDecoration(
        color: const Color(UiTokens.darkSurface),
        borderRadius: BorderRadius.circular(UiTokens.radiusCard),
        border: Border.all(color: const Color(UiTokens.darkBorder)),
      ),
      clipBehavior: Clip.antiAlias,
      child: Stack(
        fit: StackFit.expand,
        children: [
          if (renderer != null)
            RTCVideoView(renderer,
                objectFit: RTCVideoViewObjectFit.RTCVideoViewObjectFitCover)
          else
            Center(
              child: CircleAvatar(
                radius: 28,
                backgroundColor:
                    Theme.of(context).colorScheme.primary.withAlpha(35),
                child: Text(
                  _actorName(actor).characters.first.toUpperCase(),
                  style: TextStyle(
                    color: Theme.of(context).colorScheme.primary,
                    fontWeight: FontWeight.w700,
                  ),
                ),
              ),
            ),
          Positioned(
            left: 8,
            bottom: 6,
            child: Text(
              _actorName(actor),
              style: const TextStyle(fontSize: 11, color: Colors.white70),
            ),
          ),
        ],
      ),
    );
  }

  Widget _controls() {
    return Container(
      padding: const EdgeInsets.fromLTRB(16, 10, 16, 16),
      decoration: BoxDecoration(
        color: const Color(UiTokens.darkSurface),
        borderRadius: const BorderRadius.vertical(
          top: Radius.circular(UiTokens.radiusCard),
        ),
        border: Border.all(color: const Color(UiTokens.darkBorder)),
      ),
      child: Wrap(
        alignment: WrapAlignment.spaceEvenly,
        spacing: 20,
        runSpacing: 12,
        children: [
          _controlButton(
            icon: _micOn ? Icons.mic_rounded : Icons.mic_off_rounded,
            label: _micOn ? context.l10n.callMute : context.l10n.callUnmute,
            active: _micOn,
            onTap: _toggleMic,
          ),
          if (widget.mediaType == 'video')
            _controlButton(
              icon: _cameraOn
                  ? Icons.videocam_rounded
                  : Icons.videocam_off_rounded,
              label: context.l10n.callCamera,
              active: _cameraOn,
              onTap: _toggleCamera,
            ),
          // Screen sharing is a desktop affordance for now; Android calls stay
          // focused on audio/video and avoid the MediaProjection service.
          if (defaultTargetPlatform != TargetPlatform.android)
            _controlButton(
              icon: Icons.screen_share_rounded,
              label: context.l10n.callScreenShare,
              active: _screenSharing,
              onTap: _toggleScreenShare,
            ),
          _controlButton(
            icon: Icons.call_end_rounded,
            label: context.l10n.callEnd,
            danger: true,
            onTap: _end,
          ),
        ],
      ),
    );
  }

  Widget _controlButton({
    required IconData icon,
    required String label,
    required VoidCallback onTap,
    bool active = true,
    bool danger = false,
  }) {
    final color = danger
        ? const Color(0xFFFF5A6E)
        : (active
            ? Theme.of(context).colorScheme.primary
            : Theme.of(context).colorScheme.onSurfaceVariant);
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        InkWell(
          borderRadius: BorderRadius.circular(UiTokens.radiusPill),
          onTap: onTap,
          child: Container(
            width: 46,
            height: 46,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: color.withAlpha(30),
              border: Border.all(color: color.withAlpha(90)),
            ),
            child: Icon(icon, color: color, size: 22),
          ),
        ),
        const SizedBox(height: 4),
        Text(label,
            style: const TextStyle(fontSize: 10, color: Colors.white70)),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final actors = _participants.toList()..sort();
    return Scaffold(
      backgroundColor: const Color(UiTokens.darkBg),
      appBar: AppBar(
        backgroundColor: const Color(UiTokens.darkBg),
        title: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(widget.title, maxLines: 1, overflow: TextOverflow.ellipsis),
            Text(
              _connected.isEmpty
                  ? (_connecting
                      ? context.l10n.callConnecting
                      : context.l10n.callNoAnswer)
                  : '${_connected.length + 1} · ${_durationLabel()}',
              style: const TextStyle(fontSize: 11, color: Colors.white54),
            ),
          ],
        ),
        actions: [
          IconButton(
            tooltip: context.l10n.callEnd,
            onPressed: _end,
            icon: const Icon(Icons.call_end_rounded),
          ),
        ],
      ),
      body: Column(
        children: [
          if (_error != null)
            Padding(
              padding: const EdgeInsets.all(12),
              child: Text(_error!,
                  style: const TextStyle(color: Colors.redAccent)),
            ),
          Expanded(
            child: Padding(
              padding: const EdgeInsets.all(12),
              child: actors.isEmpty
                  ? Center(
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          CircleAvatar(
                            radius: 46,
                            backgroundColor: Theme.of(context)
                                .colorScheme
                                .primary
                                .withAlpha(35),
                            child: Icon(
                              widget.mediaType == 'video'
                                  ? Icons.videocam_rounded
                                  : Icons.call_rounded,
                              color: Theme.of(context).colorScheme.primary,
                              size: 34,
                            ),
                          ),
                          const SizedBox(height: 12),
                          Text(
                            _connecting
                                ? context.l10n.callWaiting
                                : context.l10n.callNoAnswer,
                            style: const TextStyle(color: Colors.white70),
                          ),
                        ],
                      ),
                    )
                  : GridView.count(
                      crossAxisCount: actors.length > 1 ? 2 : 1,
                      mainAxisSpacing: 8,
                      crossAxisSpacing: 8,
                      childAspectRatio: 16 / 10,
                      children: actors.map(_remoteTile).toList(),
                    ),
            ),
          ),
          if ((widget.mediaType == 'video' || _screenSharing) &&
              _localStream != null)
            Align(
              alignment: Alignment.centerRight,
              child: Padding(
                padding: const EdgeInsets.only(right: 16, bottom: 4),
                child: ClipRRect(
                  borderRadius: BorderRadius.circular(12),
                  child: SizedBox(
                    width: 110,
                    height: 74,
                    child: _localRenderer != null
                        ? RTCVideoView(_localRenderer!,
                            mirror: true,
                            objectFit: RTCVideoViewObjectFit
                                .RTCVideoViewObjectFitCover)
                        : const SizedBox.shrink(),
                  ),
                ),
              ),
            ),
          SafeArea(top: false, child: _controls()),
        ],
      ),
    );
  }
}
