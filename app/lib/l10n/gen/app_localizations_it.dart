// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Italian (`it`).
class AppLocalizationsIt extends AppLocalizations {
  AppLocalizationsIt([String locale = 'it']) : super(locale);

  @override
  String get appTitle => 'Fedi3';

  @override
  String get navTimeline => 'Timeline';

  @override
  String get navCompose => 'Componi';

  @override
  String get navNotifications => 'Notifiche';

  @override
  String get navSearch => 'Cerca';

  @override
  String get navChat => 'Chat';

  @override
  String get navRelays => 'Relay';

  @override
  String get navSettings => 'Configurazioni';

  @override
  String get save => 'Salva';

  @override
  String get cancel => 'Annulla';

  @override
  String get ok => 'OK';

  @override
  String err(String error) {
    return 'Errore: $error';
  }

  @override
  String get copy => 'Copia';

  @override
  String get copied => 'Copiato';

  @override
  String get enabled => 'Attivo';

  @override
  String get disabled => 'Disattivo';

  @override
  String get networkErrorTitle => 'Problema di rete';

  @override
  String get networkErrorHint => 'Controlla la connessione e riprova.';

  @override
  String get networkErrorRetry => 'Riprova';

  @override
  String get timelineTitle => 'Timeline';

  @override
  String get timelineTabHome => 'Home';

  @override
  String get timelineTabLocal => 'Locale';

  @override
  String get timelineTabSocial => 'Sociale';

  @override
  String get timelineTabFederated => 'Federata';

  @override
  String get searchTitle => 'Cerca';

  @override
  String get searchHint => 'Cerca post, utenti, hashtag';

  @override
  String get searchTabPosts => 'Post';

  @override
  String get searchTabUsers => 'Utenti';

  @override
  String get searchTabHashtags => 'Hashtag';

  @override
  String get searchEmpty => 'Scrivi per cercare';

  @override
  String get searchNoResults => 'Nessun risultato';

  @override
  String get searchShowingFor => 'Risultati per';

  @override
  String get searchSourceAll => 'Tutti';

  @override
  String get searchSourceLocal => 'Locale';

  @override
  String get searchSourceRelay => 'Relay';

  @override
  String searchTagCount(int count) {
    return '$count post';
  }

  @override
  String get chatTitle => 'Chat';

  @override
  String get chatThreadTitle => 'Conversazione';

  @override
  String get chatNewTitle => 'Nuova chat';

  @override
  String get chatNewTooltip => 'Avvia una nuova chat';

  @override
  String get chatNewMissingFields => 'Aggiungi destinatari e un messaggio.';

  @override
  String get chatNewFailed => 'Creazione chat non riuscita.';

  @override
  String get chatRecipients => 'Destinatari';

  @override
  String get chatRecipientsHint =>
      'es. @alice@example.com, https://server/users/bob';

  @override
  String get chatMessage => 'Messaggio';

  @override
  String get chatMessageHint => 'Scrivi un messaggio…';

  @override
  String get chatCreate => 'Crea';

  @override
  String get chatSend => 'Invia';

  @override
  String get chatRefresh => 'Aggiorna';

  @override
  String get chatThreadsEmpty => 'Nessuna chat';

  @override
  String get chatEmpty => 'Nessun messaggio';

  @override
  String get chatNoMessages => 'Nessun messaggio';

  @override
  String get chatDirectMessage => 'Messaggio diretto';

  @override
  String get chatGroup => 'Chat di gruppo';

  @override
  String get chatMessageDeleted => 'Messaggio eliminato';

  @override
  String get chatMessageEmpty => 'Messaggio vuoto';

  @override
  String get chatEdit => 'Modifica';

  @override
  String get chatDelete => 'Elimina';

  @override
  String get chatEditTitle => 'Modifica messaggio';

  @override
  String get chatDeleteTitle => 'Elimina messaggio';

  @override
  String get chatDeleteHint => 'Questo rimuoverà il messaggio per tutti.';

  @override
  String get chatSave => 'Salva';

  @override
  String get chatNewMessage => 'Nuovo messaggio';

  @override
  String get chatNewMessageBody => 'Hai ricevuto un nuovo messaggio.';

  @override
  String get chatOpen => 'Apri';

  @override
  String get chatGif => 'GIF';

  @override
  String get chatGifSearchHint => 'Cerca GIF…';

  @override
  String get chatGifEmpty => 'Nessuna GIF trovata';

  @override
  String get chatGifMissingKey =>
      'Aggiungi una API key GIF nelle impostazioni per abilitare la ricerca.';

  @override
  String get chatStatusPending => 'In attesa';

  @override
  String get chatStatusQueued => 'In coda';

  @override
  String get chatStatusSent => 'Inviato';

  @override
  String get chatStatusDelivered => 'Consegnato';

  @override
  String get chatStatusSeen => 'Visto';

  @override
  String get chatReply => 'Rispondi';

  @override
  String get chatReplyClear => 'Annulla risposta';

  @override
  String get chatReplyAttachment => 'Allegato';

  @override
  String get chatReplyUnknown => 'Messaggio originale';

  @override
  String get chatSenderMe => 'io';

  @override
  String get chatMembers => 'Membri';

  @override
  String get chatRename => 'Rinomina chat';

  @override
  String get chatRenameHint => 'Nuovo titolo chat';

  @override
  String get chatAddMember => 'Aggiungi membro';

  @override
  String get chatAddMemberHint => 'Cerca o incolla un actor';

  @override
  String get chatRemoveMember => 'Rimuovi membro';

  @override
  String get chatReact => 'Reagisci';

  @override
  String get chatDeleteThread => 'Elimina chat';

  @override
  String get chatDeleteThreadHint =>
      'Questo eliminerà l\'intera chat per tutti. Solo il creatore può eliminare la chat.';

  @override
  String get chatLeaveThreadOption => 'Abbandona chat';

  @override
  String get chatArchiveThreadOption => 'Archivia chat';

  @override
  String get chatLeaveThreadSuccess => 'Hai abbandonato la chat.';

  @override
  String get chatLeaveThreadFailed => 'Impossibile abbandonare la chat.';

  @override
  String get chatArchiveThreadSuccess => 'Chat archiviata.';

  @override
  String get chatArchiveThreadFailed => 'Impossibile archiviare la chat.';

  @override
  String get chatUnarchiveThreadSuccess => 'Chat ripristinata.';

  @override
  String get chatUnarchiveThreadFailed => 'Impossibile ripristinare la chat.';

  @override
  String get chatThreadsActive => 'Attive';

  @override
  String get chatThreadsArchived => 'Archiviate';

  @override
  String get chatUnarchiveThreadOption => 'Ripristina chat';

  @override
  String get chatPin => 'Fissa chat';

  @override
  String get chatUnpin => 'Rimuovi fissata';

  @override
  String chatTyping(String name) {
    return '$name sta scrivendo…';
  }

  @override
  String chatTypingMany(String names) {
    return '$names stanno scrivendo…';
  }

  @override
  String get timelineHomeTooltip =>
      'Timeline Generica (Relay + Legacy). Mostra anche interazioni in ingresso/uscita con utenti che non segui e non ti seguono.';

  @override
  String get timelineLocalTooltip =>
      'Timeline Follower/Follow (Relay + Legacy). Solo profili con cui hai relazione follow/follower.';

  @override
  String get timelineSocialTooltip =>
      'Timeline Sociale (Relay). Feed globale best-effort via relay.';

  @override
  String get timelineFederatedTooltip =>
      'Timeline Federata (Relay + Legacy). Mostra contenuti pubblici visti dalla tua istanza nella federazione.';

  @override
  String get timelineLayoutColumns => 'Passa alle colonne';

  @override
  String get timelineLayoutTabs => 'Passa alle tab';

  @override
  String get timelineFilters => 'Filtri';

  @override
  String get timelineFilterMedia => 'Media';

  @override
  String get timelineFilterReply => 'Risposte';

  @override
  String get timelineFilterBoost => 'Boost';

  @override
  String get timelineFilterMention => 'Menzioni';

  @override
  String get coreStart => 'Avvia core';

  @override
  String get coreStop => 'Ferma core';

  @override
  String get listNoItems => 'Nessun elemento';

  @override
  String get listEnd => 'Fine';

  @override
  String get listLoadMore => 'Carica altro';

  @override
  String timelineNewPosts(int count) {
    return '$count nuovi post';
  }

  @override
  String get timelineShowNewPosts => 'Mostra';

  @override
  String get timelineShowNewPostsHint => 'Clicca per mostrare senza ricaricare';

  @override
  String get timelineRefreshTitle => 'Aggiorna';

  @override
  String get timelineRefreshHint =>
      'Forza ricarica e pull dai legacy mentre eri offline';

  @override
  String get activityBoost => 'Boost';

  @override
  String get activityReply => 'Rispondi';

  @override
  String get activityLike => 'Mi piace';

  @override
  String get activityReact => 'Reazione';

  @override
  String get activityReplyTitle => 'Rispondi';

  @override
  String get activityReplyHint => 'Scrivi una risposta…';

  @override
  String get activityCancel => 'Annulla';

  @override
  String get activitySend => 'Invia';

  @override
  String get activityUnsupported => 'Elemento non supportato';

  @override
  String get activityViewRaw => 'Mostra raw';

  @override
  String get activityHideRaw => 'Nascondi raw';

  @override
  String get noteInReplyTo => 'In risposta a';

  @override
  String noteBoostedBy(String name) {
    return 'Boost di $name';
  }

  @override
  String get noteThreadTitle => 'Thread';

  @override
  String get noteThreadUp => 'In risposta a';

  @override
  String get noteShowThread => 'Mostra thread';

  @override
  String get noteHideThread => 'Nascondi thread';

  @override
  String get noteLoadingThread => 'Caricamento…';

  @override
  String get noteShowReplies => 'Mostra risposte';

  @override
  String get noteHideReplies => 'Nascondi risposte';

  @override
  String get noteLoadingReplies => 'Caricamento risposte…';

  @override
  String get noteReactionLoading => 'Caricamento reazioni…';

  @override
  String get noteReactionAdd => 'Aggiungi reazione';

  @override
  String get noteContentWarning => 'Contenuto sensibile';

  @override
  String get noteShowContent => 'Mostra';

  @override
  String get noteHideContent => 'Nascondi';

  @override
  String get noteActions => 'Azioni';

  @override
  String get noteEdit => 'Modifica';

  @override
  String get noteDelete => 'Elimina';

  @override
  String get noteDeleted => 'Nota eliminata';

  @override
  String get noteEditTitle => 'Modifica nota';

  @override
  String get noteEditContentHint => 'Modifica contenuto';

  @override
  String get noteEditSummaryHint => 'Avviso contenuto (opzionale)';

  @override
  String get noteSensitiveLabel => 'Contenuto sensibile';

  @override
  String get noteEditSave => 'Salva';

  @override
  String get noteEditMissingAudience => 'Destinatario mancante';

  @override
  String get noteDeleteTitle => 'Elimina nota';

  @override
  String get noteDeleteHint => 'Sei sicuro di voler eliminare questa nota?';

  @override
  String get noteDeleteConfirm => 'Elimina';

  @override
  String get noteDeleteMissingAudience => 'Destinatario mancante';

  @override
  String get timeAgoJustNow => 'adesso';

  @override
  String timeAgoMinutes(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count minuti fa',
      one: '1 minuto fa',
    );
    return '$_temp0';
  }

  @override
  String timeAgoHours(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count ore fa',
      one: '1 ora fa',
    );
    return '$_temp0';
  }

  @override
  String timeAgoDays(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count giorni fa',
      one: '1 giorno fa',
    );
    return '$_temp0';
  }

  @override
  String timeAgoMonths(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count mesi fa',
      one: '1 mese fa',
    );
    return '$_temp0';
  }

  @override
  String timeAgoYears(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count anni fa',
      one: '1 anno fa',
    );
    return '$_temp0';
  }

  @override
  String get firstRunTitle => 'Benvenuto';

  @override
  String get firstRunIntro =>
      'Vuoi fare il login con un account esistente oppure crearne uno nuovo?';

  @override
  String get firstRunLoginTitle => 'Login (config esistente)';

  @override
  String get firstRunLoginHint =>
      'Inserisci i dati di un account/setup già esistente.';

  @override
  String get firstRunCreateTitle => 'Crea nuovo account';

  @override
  String get firstRunCreateHint =>
      'Crea una nuova identità locale e configura gli endpoint del relay.';

  @override
  String get firstRunRelayPreviewTitle => 'Anteprima rete relay';

  @override
  String get firstRunRelayPreviewRelays => 'Relay';

  @override
  String get firstRunRelayPreviewPeers => 'Peer';

  @override
  String get firstRunRelayPreviewEmpty => 'Nessun dato disponibile';

  @override
  String get firstRunRelayPreviewError =>
      'Impossibile caricare l\'anteprima relay.';

  @override
  String get onboardingCreateTitle => 'Crea account';

  @override
  String get onboardingCreateIntro =>
      'Compila i campi necessari. Il core verrà avviato automaticamente dopo il salvataggio.';

  @override
  String get onboardingLoginTitle => 'Login';

  @override
  String get onboardingLoginIntro =>
      'Inserisci i campi del tuo setup esistente. Il core verrà avviato automaticamente dopo il salvataggio.';

  @override
  String get onboardingImportBackup => 'Importa backup JSON';

  @override
  String get onboardingImportBackupCloud => 'Ripristina dal relay';

  @override
  String get onboardingRelaySelect => 'Selezione relay';

  @override
  String get onboardingRelayDiscover => 'Scopri';

  @override
  String get onboardingRelayDiscovering => 'Ricerca…';

  @override
  String get onboardingRelayListHint =>
      'Scegli un relay dalla lista condivisa o inseriscilo manualmente.';

  @override
  String get onboardingRelayPick => 'Seleziona un relay';

  @override
  String get onboardingRelayCustom => 'Usa impostazioni relay personalizzate';

  @override
  String get composeTitle => 'Componi';

  @override
  String composeCharCount(int used, int max) {
    return '$used/$max';
  }

  @override
  String composeCoreNotRunning(String settings, String start) {
    return 'Core non avviato: vai in $settings e premi $start.';
  }

  @override
  String get composeWhatsHappening => 'Scrivi cosa pensi.';

  @override
  String get composePublic => 'Pubblico';

  @override
  String get composePublicHint => 'Se off: solo follower';

  @override
  String get composeAddMediaPath => 'Aggiungi media (path)';

  @override
  String get composeAddMedia => 'Allega media';

  @override
  String get composeDropHere => 'Trascina qui i file per allegare';

  @override
  String get composeClearDraft => 'Svuota bozza';

  @override
  String get composeDraftSaved => 'Bozza salvata';

  @override
  String get composeDraftRestored => 'Bozza ripristinata';

  @override
  String get composeDraftCleared => 'Bozza eliminata';

  @override
  String get composeDraftResumeTooltip => 'Riprendi bozza';

  @override
  String get composeDraftDeleteTooltip => 'Elimina bozza';

  @override
  String get composeQuickHint => 'Apri il compositore rapido';

  @override
  String get translationTitle => 'Traduzione';

  @override
  String get translationHint => 'Impostazioni DeepL o DeepLX';

  @override
  String get translationProviderLabel => 'Provider';

  @override
  String get translationProviderDeepL => 'DeepL';

  @override
  String get translationProviderDeepLX => 'DeepLX';

  @override
  String get translationAuthKeyLabel => 'Chiave API';

  @override
  String get translationAuthKeyHint => 'Chiave API DeepL';

  @override
  String get translationUseProLabel => 'Account Pro';

  @override
  String get translationUseProHint =>
      'Usa api.deepl.com invece di api-free.deepl.com';

  @override
  String get translationDeepLXUrlLabel => 'Endpoint DeepLX';

  @override
  String get translationDeepLXUrlHint => 'https://api.deeplx.org/translate';

  @override
  String get translationTimeoutLabel => 'Timeout';

  @override
  String translationTimeoutValue(int seconds) {
    String _temp0 = intl.Intl.pluralLogic(
      seconds,
      locale: localeName,
      other: '# secondi',
      one: '# secondo',
    );
    return '$_temp0';
  }

  @override
  String translationTargetLabel(String lang) {
    return 'Lingua di destinazione: $lang';
  }

  @override
  String get noteTranslate => 'Traduci';

  @override
  String get noteShowTranslation => 'Mostra traduzione';

  @override
  String get noteShowOriginal => 'Mostra originale';

  @override
  String noteTranslatedFrom(String lang) {
    return 'Tradotto da $lang';
  }

  @override
  String noteTranslateFailed(String error) {
    return 'Traduzione non riuscita: $error';
  }

  @override
  String get composePost => 'Pubblica';

  @override
  String get composeAttachments => 'Allegati';

  @override
  String composeMediaId(String id) {
    return 'mediaId=$id';
  }

  @override
  String get composeFileFallback => 'file';

  @override
  String get composeMediaFilePathLabel => 'Percorso file media (desktop/dev)';

  @override
  String get composeMediaFilePathHint =>
      'C:\\\\path\\\\img.png oppure /home/user/img.png';

  @override
  String get composeNotUploaded => 'Non caricato';

  @override
  String get composeQueuedOk => 'OK: in coda per la consegna';

  @override
  String get composeErrEmptyContent => 'ERR: contenuto vuoto';

  @override
  String composeErrUnableReadFile(String error) {
    return 'ERR: impossibile leggere il file: $error';
  }

  @override
  String composeErrUnablePickFile(String error) {
    return 'ERR: selezione file fallita: $error';
  }

  @override
  String composeErrInvalidMediaType(String type) {
    return 'ERR: tipo media non valido: $type';
  }

  @override
  String composeErrGeneric(String error) {
    return 'ERR: $error';
  }

  @override
  String get settingsTitle => 'Configurazioni';

  @override
  String get settingsCore => 'Core';

  @override
  String settingsCoreRunning(int handle) {
    return 'In esecuzione (handle=$handle)';
  }

  @override
  String get settingsCoreStopped => 'Fermo';

  @override
  String get settingsAccount => 'Account';

  @override
  String get settingsFollowSection => 'Follow / Unfollow';

  @override
  String get settingsActorUrlLabel => 'Actor URL (https://...)';

  @override
  String get settingsFollow => 'Follow';

  @override
  String get settingsUnfollow => 'Unfollow';

  @override
  String get settingsAdvancedDev => 'Avanzate (dev)';

  @override
  String get settingsAdvancedDevHint => 'Start/Stop + status migrazione';

  @override
  String get settingsResetApp => 'Reset app (cancella config)';

  @override
  String get profileEditTitle => 'Profilo';

  @override
  String get profileEditHint => 'Profilo pubblico, avatar, banner, campi';

  @override
  String get profileDisplayName => 'Nome visualizzato';

  @override
  String get profileBio => 'Bio';

  @override
  String get profileFollowers => 'Follower';

  @override
  String get profileFollowing => 'Seguiti';

  @override
  String get profileFeatured => 'In evidenza';

  @override
  String get profileAliases => 'Alias';

  @override
  String get profileFollowPending => 'In attesa';

  @override
  String profileMovedTo(String actor) {
    return 'Spostato su $actor';
  }

  @override
  String get profileAvatar => 'Avatar';

  @override
  String get profileBanner => 'Banner';

  @override
  String get profilePickFile => 'Scegli file';

  @override
  String get profileFilePathHint => 'Percorso file (desktop)';

  @override
  String get profileUpload => 'Carica';

  @override
  String get profileUploadOk => 'OK: caricato';

  @override
  String get profileSave => 'Salva';

  @override
  String get profileSavedOk => 'OK: salvato (core riavviato)';

  @override
  String profileErrSave(String error) {
    return 'Salvataggio fallito: $error';
  }

  @override
  String profileErrUpload(String error) {
    return 'Upload fallito: $error';
  }

  @override
  String get profileErrCoreNotRunning => 'Core non avviato.';

  @override
  String get profileFieldsTitle => 'Campi profilo';

  @override
  String get profileFieldAdd => 'Aggiungi campo';

  @override
  String get profileFieldEdit => 'Modifica campo';

  @override
  String get profileFieldName => 'Nome';

  @override
  String get profileFieldValue => 'Valore';

  @override
  String get profileFieldNameEmpty => '(vuoto)';

  @override
  String get privacyTitle => 'Privacy';

  @override
  String get privacyHint => 'Impostazioni privacy account';

  @override
  String get privacyLockedAccount =>
      'Account privato (approvazione follow manuale)';

  @override
  String get privacyLockedAccountHint =>
      'Se attivo, le richieste di follow richiedono approvazione.';

  @override
  String get telemetrySectionTitle => 'Diagnostica';

  @override
  String get telemetryEnabled => 'Telemetria anonima';

  @override
  String get telemetryEnabledHint =>
      'Condividi diagnostica minima (errori e prestazioni) per migliorare la stabilità. Nessun contenuto personale.';

  @override
  String get telemetryMonitoringEnabled =>
      'Monitoraggio client (debug/staging)';

  @override
  String get telemetryMonitoringHint =>
      'Mantieni un log diagnostico locale per il troubleshooting.';

  @override
  String get telemetryOpen => 'Apri diagnostica';

  @override
  String get telemetryTitle => 'Diagnostica';

  @override
  String get telemetryRefresh => 'Aggiorna';

  @override
  String get telemetryExport => 'Esporta';

  @override
  String get telemetryExportEmpty => 'Nessuna diagnostica da esportare';

  @override
  String get telemetryClear => 'Svuota';

  @override
  String get telemetryEmpty => 'Nessuna diagnostica';

  @override
  String updateAvailable(String version) {
    return 'Aggiornamento disponibile: $version';
  }

  @override
  String get updateInstall => 'Installa aggiornamento';

  @override
  String get updateDownloading => 'Download in corso...';

  @override
  String updateFailed(String error) {
    return 'Aggiornamento fallito: $error';
  }

  @override
  String get updateChangelog => 'Changelog';

  @override
  String get updateDismiss => 'Chiudi';

  @override
  String get updateOpenRelease => 'Apro la pagina release';

  @override
  String get securityTitle => 'Sicurezza';

  @override
  String get securityHint => 'Token e endpoint interni';

  @override
  String get securityInternalToken => 'Token interno';

  @override
  String get securityInternalTokenHint =>
      'Usato per proteggere gli endpoint interni';

  @override
  String get securityRegenerate => 'Rigenera';

  @override
  String get securityHintInternalEndpoints =>
      'Tieni questo token privato. Se esposto, altri nella tua rete potrebbero chiamare endpoint interni.';

  @override
  String get moderationTitle => 'Moderazione';

  @override
  String get moderationHintTitle => 'Liste di blocco e policy';

  @override
  String get moderationBlockedDomains => 'Domini bloccati';

  @override
  String get moderationBlockedDomainsHint =>
      'Uno per riga (example.com o *.example.com)';

  @override
  String get moderationBlockedActors => 'Actor bloccati';

  @override
  String get moderationBlockedActorsHint => 'Un URL actor per riga';

  @override
  String get moderationHint =>
      'I blocchi si applicano a interazioni in ingresso e in uscita.';

  @override
  String get networkingTitle => 'Networking';

  @override
  String get networkingHintTitle => 'Relay e AP relays';

  @override
  String get networkingRelay => 'Relay public base URL';

  @override
  String get networkingRelayWs => 'Relay WebSocket';

  @override
  String get networkingBind => 'Bind locale';

  @override
  String get networkingRelaysTitle => 'Discovery relay';

  @override
  String networkingRelaysCount(int count) {
    return '$count relay noti';
  }

  @override
  String get networkingRelaysEmpty => 'Nessun relay';

  @override
  String get relayAdminTitle => 'Relay admin';

  @override
  String get relayAdminHint => 'Gestisci utenti relay e audit';

  @override
  String get relayAdminRelayWsLabel => 'Relay WS';

  @override
  String get relayAdminTokenLabel => 'Token admin';

  @override
  String get relayAdminTokenHint => 'Incolla token admin';

  @override
  String get relayAdminTokenMissing =>
      'Aggiungi un token admin per usare il relay admin.';

  @override
  String get relayAdminUsers => 'Utenti';

  @override
  String get relayAdminAudit => 'Audit';

  @override
  String get relayAdminRegister => 'Registra';

  @override
  String get relayAdminRegisterHint => 'username (es. alice)';

  @override
  String get relayAdminUsername => 'Username';

  @override
  String get relayAdminGenerateToken => 'Genera token';

  @override
  String get relayAdminRotate => 'Ruota token';

  @override
  String get relayAdminEnable => 'Abilita';

  @override
  String get relayAdminDisable => 'Disabilita';

  @override
  String get relayAdminDelete => 'Elimina';

  @override
  String get relayAdminAuditFailed => 'Fallito';

  @override
  String get relayAdminUserEnabled => 'Attivo';

  @override
  String get relayAdminUserDisabled => 'Disattivo';

  @override
  String get relayAdminUserSearchHint => 'Cerca utenti…';

  @override
  String relayAdminDeleteConfirm(String username) {
    return 'Eliminare l\'utente $username?';
  }

  @override
  String get relayAdminAuditExport => 'Esporta audit';

  @override
  String relayAdminAuditExported(String path) {
    return 'Audit esportato: $path';
  }

  @override
  String get relayAdminAuditSearchHint => 'Cerca audit…';

  @override
  String get relayAdminAuditFailedOnly => 'Solo falliti';

  @override
  String get relayAdminAuditReverse => 'Ordine inverso';

  @override
  String relayAdminUsersCount(int count) {
    return 'Utenti: $count';
  }

  @override
  String relayAdminUsersDisabledCount(int count) {
    return 'Disattivi: $count';
  }

  @override
  String get relayAdminAuditLast => 'Ultimo audit';

  @override
  String get networkingRelaysError => 'Sync relay non riuscita';

  @override
  String get networkingApRelays => 'ActivityPub relays';

  @override
  String get networkingApRelaysEmpty => 'Nessun AP relay configurato';

  @override
  String get networkingEditAccount => 'Modifica account/networking';

  @override
  String get networkingHint =>
      'Alcune modifiche richiedono il riavvio del core.';

  @override
  String get p2pSectionTitle => 'Consegna P2P';

  @override
  String get p2pDeliveryModeLabel => 'Modalita consegna';

  @override
  String get p2pDeliveryModeRelay => 'P2P prima, fallback al relay';

  @override
  String get p2pDeliveryModeP2POnly => 'Solo P2P (no relay)';

  @override
  String get p2pRelayFallbackLabel => 'Ritardo fallback relay (secondi)';

  @override
  String get p2pRelayFallbackHint =>
      'Attendi prima di usare il relay (default: 5)';

  @override
  String get p2pCacheTtlLabel => 'TTL cache mailbox (secondi)';

  @override
  String get p2pCacheTtlHint => 'TTL store-and-forward (default: 604800)';

  @override
  String get backupTitle => 'Backup';

  @override
  String get backupHint => 'Esporta/importa profilo e impostazioni Fedi3';

  @override
  String get backupExportTitle => 'Esporta';

  @override
  String get backupExportHint =>
      'Crea un backup JSON (config + preferenze UI).';

  @override
  String get backupExportSave => 'Salva file backup';

  @override
  String get backupExportCopy => 'Copia JSON backup';

  @override
  String get backupExportOk => 'OK: copiato negli appunti';

  @override
  String get backupExportSaved => 'OK: file backup salvato';

  @override
  String get backupImportTitle => 'Importa';

  @override
  String get backupImportHint =>
      'Incolla qui un backup JSON esportato in precedenza.';

  @override
  String get backupImportApply => 'Importa ora';

  @override
  String get backupImportFile => 'Importa da file';

  @override
  String get backupImportOk => 'OK: importato (core riavviato)';

  @override
  String get backupCloudTitle => 'Backup cloud';

  @override
  String get backupCloudHint =>
      'Backup cifrato salvato sul relay (o S3) per sincronizzare i dispositivi.';

  @override
  String get backupCloudUpload => 'Carica sul relay';

  @override
  String get backupCloudDownload => 'Ripristina dal relay';

  @override
  String get backupCloudUploadOk => 'OK: backup caricato';

  @override
  String get backupCloudDownloadOk => 'OK: backup ripristinato';

  @override
  String backupErr(String error) {
    return 'Errore backup: $error';
  }

  @override
  String get statusRelay => 'Relay';

  @override
  String get statusMailbox => 'Mailbox';

  @override
  String get statusRelayRtt => 'R';

  @override
  String get statusMailboxRtt => 'M';

  @override
  String get statusRelayTraffic => 'R ↑/↓';

  @override
  String get statusMailboxTraffic => 'M ↑/↓';

  @override
  String get statusCoreStoppedShort => 'core off';

  @override
  String get statusUnknownShort => '?';

  @override
  String get statusConnectedShort => 'on';

  @override
  String get statusDisconnectedShort => 'off';

  @override
  String get statusNoPeersShort => '0/0';

  @override
  String get settingsOk => 'OK';

  @override
  String settingsErr(String error) {
    return 'ERR: $error';
  }

  @override
  String get relaysTitle => 'Relay';

  @override
  String get relaysCurrent => 'Relay attuale';

  @override
  String get relaysTelemetry => 'Telemetria';

  @override
  String get relaysKnown => 'Relay conosciuti';

  @override
  String relaysLastSeen(String ms) {
    return 'last_seen=$ms';
  }

  @override
  String get relaysPeersTitle => 'Peer conosciuti';

  @override
  String get relaysPeersSearchHint => 'Cerca peer';

  @override
  String get relaysPeersEmpty => 'Nessun peer trovato';

  @override
  String get relaysRecommended => 'Consigliato';

  @override
  String relaysLatency(int ms) {
    return 'Latenza: $ms ms';
  }

  @override
  String get relaysCoverageTitle => 'Copertura ricerca';

  @override
  String relaysCoverageUsers(int indexed, int total) {
    return '$indexed/$total utenti indicizzati';
  }

  @override
  String relaysCoverageLast(String ms) {
    return 'Ultima indicizzazione: $ms';
  }

  @override
  String get onboardingTitle => 'Setup Fedi3';

  @override
  String get onboardingIntro =>
      'Crea la tua istanza locale e collegala a un relay (per compatibilità con le istanze legacy).';

  @override
  String get onboardingUsername => 'Username';

  @override
  String get onboardingDomain => 'Domain (handle: user@domain)';

  @override
  String get onboardingRelayPublicUrl =>
      'Relay public URL (https://relay... oppure http://127.0.0.1:8787)';

  @override
  String get onboardingRelayWs =>
      'Relay WS (wss://... oppure ws://127.0.0.1:8787)';

  @override
  String get onboardingRelayToken => 'Relay token';

  @override
  String get onboardingBind => 'Local bind (host:port)';

  @override
  String get onboardingInternalToken => 'Internal token (UI ↔ core)';

  @override
  String get onboardingSave => 'Salva e apri app';

  @override
  String get onboardingRelayTokenTooShort =>
      'Relay token troppo corto (min 16 caratteri).';

  @override
  String get editAccountTitle => 'Modifica account';

  @override
  String get editAccountSave => 'Salva';

  @override
  String get editAccountCoreRunningWarning =>
      'Core in esecuzione: verrà fermato automaticamente prima di salvare per evitare incoerenze.';

  @override
  String get editAccountRelayPublicUrl =>
      'Relay public URL (https://relay.fedi3.com)';

  @override
  String get editAccountRelayWs => 'Relay WS (wss://relay.fedi3.com)';

  @override
  String get editAccountRegenerateInternal => 'Rigenera internal token';

  @override
  String get devCoreTitle => 'Dev core controls';

  @override
  String get devCoreConfigSaved => 'Config (salvata)';

  @override
  String get devCoreStart => 'Start';

  @override
  String get devCoreStop => 'Stop';

  @override
  String get devCoreFetchMigration => 'Leggi status migrazione';

  @override
  String devCoreVersion(String version) {
    return 'Versione core: $version';
  }

  @override
  String devCoreNotLoaded(String error) {
    return 'Core non caricato: $error';
  }

  @override
  String get notificationsTitle => 'Notifiche';

  @override
  String get notificationsEmpty => 'Nessuna notifica';

  @override
  String get notificationsCoreNotRunning => 'Core non avviato.';

  @override
  String get notificationsGeneric => 'Notifica';

  @override
  String get notificationsFollow => 'Nuovo follower';

  @override
  String get notificationsFollowAccepted => 'Follow accettato';

  @override
  String get notificationsFollowRejected => 'Follow rifiutato';

  @override
  String get notificationsLike => 'Ha messo Mi piace';

  @override
  String get notificationsReact => 'Ha reagito al post';

  @override
  String get notificationsBoost => 'Ha boostato il post';

  @override
  String get notificationsMentionOrReply => 'Menzione / risposta';

  @override
  String get notificationsNewActivity => 'Nuova attività';

  @override
  String get uiSettingsTitle => 'Aspetto e lingua';

  @override
  String get uiSettingsHint => 'Tema, lingua, densità, dimensione testo';

  @override
  String get uiLanguage => 'Lingua';

  @override
  String get uiLanguageSystem => 'Default sistema';

  @override
  String get uiTheme => 'Tema';

  @override
  String get uiThemeSystem => 'Sistema';

  @override
  String get uiThemeLight => 'Chiaro';

  @override
  String get uiThemeDark => 'Scuro';

  @override
  String get uiDensity => 'Densità';

  @override
  String get uiDensityNormal => 'Normale';

  @override
  String get uiDensityCompact => 'Compatta';

  @override
  String get uiAccent => 'Colore accento';

  @override
  String get uiFontSize => 'Dimensione testo';

  @override
  String get uiFontSizeHint => 'Influisce su tutta l\'app';

  @override
  String get gifSettingsTitle => 'GIF';

  @override
  String get gifSettingsHint => 'API key Giphy';

  @override
  String get gifProviderLabel => 'Provider';

  @override
  String get gifProviderHint =>
      'Usa una API key personale per abilitare la ricerca.';

  @override
  String get gifProviderTenor => 'Tenor';

  @override
  String get gifProviderGiphy => 'Giphy';

  @override
  String get gifApiKeyLabel => 'API key';

  @override
  String get gifApiKeyHint => 'Incolla la API key di Giphy';

  @override
  String get gifSettingsDefaultHint =>
      'Puoi usare la key predefinita di Giphy per ora.';

  @override
  String get gifSettingsUseDefault => 'Usa key predefinita';

  @override
  String get composeContentWarningTitle => 'Content warning';

  @override
  String get composeContentWarningHint =>
      'Nascondi il contenuto dietro un avviso';

  @override
  String get composeContentWarningTextLabel => 'Testo avviso';

  @override
  String get composeSensitiveMediaTitle => 'Media sensibili';

  @override
  String get composeSensitiveMediaHint => 'Segna i media come sensibili';

  @override
  String get composeEmojiButton => 'Emoji';

  @override
  String get composeMfmCheatsheet => 'MFM cheatsheet';

  @override
  String get composeMfmCheatsheetTitle => 'MFM cheatsheet';

  @override
  String get composeMfmCheatsheetBody =>
      '**grassetto** -> grassetto\n*corsivo* -> corsivo\n~~barrato~~ -> barrato\n`codice` -> codice in linea\n```codice``` -> blocco di codice\n> citazione -> citazione\n[titolo](https://esempio.com) -> link\n#tag -> hashtag\n@utente@dominio -> menzione\n:emoji: -> emoji personalizzata\nA capo -> nuova linea';

  @override
  String get close => 'Chiudi';

  @override
  String get composeVisibilityTitle => 'Visibilità';

  @override
  String get composeVisibilityHint => 'Chi può vedere questo post';

  @override
  String get composeVisibilityPublic => 'Pubblico';

  @override
  String get composeVisibilityHome => 'Home';

  @override
  String get composeVisibilityFollowers => 'Solo follower';

  @override
  String get composeVisibilityDirect => 'Diretto';

  @override
  String get composeVisibilityDirectLabel => 'Destinatario diretto';

  @override
  String get composeVisibilityDirectHint => '@utente@host o URL attore';

  @override
  String get composeVisibilityDirectMissing => 'Serve un destinatario diretto';

  @override
  String get composeExpand => 'Espandi';

  @override
  String get composeExpandTitle => 'Editor';

  @override
  String get uiEmojiPaletteTitle => 'Palette emoji';

  @override
  String get uiEmojiPaletteHint => 'Modifica emoji rapide';

  @override
  String get emojiPaletteAddLabel => 'Aggiungi emoji';

  @override
  String get emojiPaletteAddHint => '😀 o :shortcode:';

  @override
  String get emojiPaletteAddButton => 'Aggiungi';

  @override
  String get emojiPaletteEmpty => 'Nessuna emoji.';

  @override
  String get emojiPickerTitle => 'Emoji';

  @override
  String get emojiPickerClose => 'Chiudi';

  @override
  String get emojiPickerSearchLabel => 'Cerca';

  @override
  String get emojiPickerSearchHint => 'Emoji o :shortcode:';

  @override
  String get emojiPickerPalette => 'Palette';

  @override
  String get emojiPickerRecent => 'Recenti';

  @override
  String get emojiPickerCommon => 'Comuni';

  @override
  String get emojiPickerCustom => 'Emoji custom';

  @override
  String get reactionPickerTitle => 'Reazioni';

  @override
  String get reactionPickerClose => 'Chiudi';

  @override
  String get reactionPickerSearchLabel => 'Cerca o personalizza';

  @override
  String get reactionPickerSearchHint => 'Emoji o :shortcode:';

  @override
  String get reactionPickerRecent => 'Recenti';

  @override
  String get reactionPickerCommon => 'Comuni';

  @override
  String get reactionPickerNoteEmojis => 'Emoji della nota';

  @override
  String get reactionPickerGlobalEmojis => 'Emoji globali';

  @override
  String get uiEmojiPickerTitle => 'Selettore emoji';

  @override
  String get uiEmojiPickerSizeLabel => 'Dimensione';

  @override
  String get uiEmojiPickerColumnsLabel => 'Colonne';

  @override
  String get uiEmojiPickerStyleLabel => 'Stile emoji custom';

  @override
  String get uiEmojiPickerStyleImage => 'Immagine';

  @override
  String get uiEmojiPickerStyleText => 'Testo';

  @override
  String get uiEmojiPickerPresetLabel => 'Preset';

  @override
  String get uiEmojiPickerPresetCompact => 'Compatto';

  @override
  String get uiEmojiPickerPresetComfort => 'Comodo';

  @override
  String get uiEmojiPickerPresetLarge => 'Grande';

  @override
  String get uiEmojiPickerPreviewLabel => 'Anteprima';

  @override
  String get uiNotificationsTitle => 'Notifiche';

  @override
  String get uiNotificationsChat => 'Messaggi chat';

  @override
  String get uiNotificationsDirect => 'Interazioni dirette';
}
