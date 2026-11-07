import 'package:flutter/material.dart';
import '../../services/mobile_push_service.dart';
import '../../l10n/l10n_ext.dart';

class MobilePushSettings extends StatefulWidget {
  const MobilePushSettings({super.key});
  @override
  State<MobilePushSettings> createState() => _MobilePushSettingsState();
}

class _MobilePushSettingsState extends State<MobilePushSettings> {
  bool busy = false;
  String? error;
  @override
  Widget build(BuildContext context) {
    final service = MobilePushService.instance;
    final it = context.l10n.localeName == 'it';
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      SwitchListTile(
          title: Text(it ? 'Notifiche push' : 'Push notifications'),
          subtitle: Text(it
              ? 'UnifiedPush su Android, APNs su iOS. Richiede un provider configurato sul relay.'
              : 'UnifiedPush on Android, APNs on iOS. Requires a configured relay provider.'),
          value: service.enabled,
          onChanged: busy ? null : (value) => _change(value)),
      ValueListenableBuilder(
          valueListenable: service.status,
          builder: (context, value, _) => Text(_label(value, it))),
      if (error != null)
        Text(error!,
            style: TextStyle(color: Theme.of(context).colorScheme.error)),
      if (busy) const LinearProgressIndicator(),
    ]);
  }

  String _label(String value, bool it) {
    switch (value) {
      case 'registered':
        return it ? 'Registrazione attiva' : 'Registration active';
      case 'registering':
        return it ? 'Registrazione in corso…' : 'Registering…';
      case 'disabled':
        return it ? 'Disattivate' : 'Disabled';
      case 'relay_registration_pending':
        return it
            ? 'Rinnovo sul relay in attesa. Riproverà alla riapertura.'
            : 'Relay renewal pending. Retries when the app resumes.';
      case 'registration_vapidRequired':
        return it
            ? 'Il distributore richiede una chiave VAPID sul relay.'
            : 'Distributor requires a relay VAPID key.';
      case 'revocation_pending':
        return it
            ? 'Revoca sul relay in attesa. Riprova prima di cambiare account.'
            : 'Relay revocation pending. Retry before switching accounts.';
      case '':
        return '';
      default:
        return it
            ? 'Servizio temporaneamente non disponibile. Riprova.'
            : 'Service temporarily unavailable. Retry.';
    }
  }

  Future<void> _change(bool value) async {
    setState(() {
      busy = true;
      error = null;
    });
    try {
      final service = MobilePushService.instance;
      if (value) {
        final distributors = await service.distributors();
        String? selected;
        if (distributors.isNotEmpty && mounted) {
          selected = await showDialog<String>(
              context: context,
              builder: (context) => SimpleDialog(
                  title: Text(context.l10n.localeName == 'it'
                      ? 'Scegli distributore'
                      : 'Choose distributor'),
                  children: distributors
                      .map((name) => SimpleDialogOption(
                          onPressed: () => Navigator.pop(context, name),
                          child: Text(name)))
                      .toList()));
          if (selected == null) return;
        }
        await service.enable(distributor: selected);
      } else {
        await service.disable();
      }
    } catch (_) {
      if (mounted) {
        error = context.l10n.localeName == 'it'
            ? 'Registrazione non riuscita: verifica il distributore e la configurazione push del relay.'
            : 'Registration failed: check the distributor and relay push configuration.';
      }
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }
}
