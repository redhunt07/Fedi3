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
  String get settingsRelaysHint =>
      'Scegli i relay, scopri i peer e controlla la connettività.';

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
  String get coreAuthErrorTitle => 'Autenticazione del core non riuscita';

  @override
  String get coreAuthErrorHint =>
      'Verifica il token interno nelle impostazioni di sicurezza. Se hai avviato un core di sviluppo separato, usa lo stesso token oppure arrestalo e avvia il core dall’app.';

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
  String get timelineTabFollowing => 'Seguiti';

  @override
  String get timelineTabDiscover => 'Scopri';

  @override
  String get coreOnline => 'Core online';

  @override
  String get coreOffline => 'Core offline';

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
  String get searchRecent => 'Ricerche recenti';

  @override
  String get searchClearHistory => 'Cancella cronologia';

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
  String get chatE2eBadge => 'Cifrata end-to-end';

  @override
  String get announcementsTitle => 'Annunci';

  @override
  String get announcementsSubtitle => 'Aggiornamenti ufficiali dai maintainer';

  @override
  String get announcementsEmpty => 'Nessun annuncio disponibile.';

  @override
  String get announcementsRefresh => 'Aggiorna';

  @override
  String get announcementsNewAlert => 'Nuovi annunci dai maintainer';

  @override
  String get announcementsOpen => 'Apri';

  @override
  String get announcementsOffline =>
      'Impossibile raggiungere il repository: mostro l’ultima copia salvata.';

  @override
  String get announcementsSource => 'Sorgente Git';

  @override
  String announcementsLastSync(String when) {
    return 'Ultimo controllo: $when';
  }

  @override
  String get announcementsMarkRead => 'Segna come letti';

  @override
  String get chatEmptyHint =>
      'Le conversazioni cifrate end-to-end appariranno qui.';

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
  String get chatNewDmTab => 'Messaggio';

  @override
  String get chatNewGroupTab => 'Gruppo';

  @override
  String get chatNewSearchHint => 'Cerca tra i contatti reciproci';

  @override
  String get chatNewMutualEmpty => 'Nessun contatto reciproco';

  @override
  String get chatNewMutualHint =>
      'Segui qualcuno e fatti seguire per iniziare una chat.';

  @override
  String get chatNewSortTitle => 'Ordina';

  @override
  String get chatNewSortInteractions => 'Interazioni';

  @override
  String get chatNewSortAlpha => 'A-Z';

  @override
  String chatNewInteractions(int count) {
    return 'Interazioni: $count';
  }

  @override
  String get chatNewGroupName => 'Nome del gruppo';

  @override
  String get chatNewGroupNameHint => 'Es. Team progetto';

  @override
  String get chatNewCreateGroup => 'Crea gruppo';

  @override
  String chatNewSelectedCount(int count) {
    return '$count selezionati';
  }

  @override
  String get chatNewGroupDefault => 'Gruppo';

  @override
  String get chatRoleOwner => 'Proprietario';

  @override
  String get chatRoleAdmin => 'Amministratore';

  @override
  String get chatRoleMember => 'Membro';

  @override
  String get chatRoleChange => 'Cambia ruolo';

  @override
  String get callAudio => 'Chiamata audio';

  @override
  String get callVideo => 'Chiamata video';

  @override
  String get callIncoming => 'Chiamata in arrivo';

  @override
  String get callIncomingVideo => 'Videochiamata';

  @override
  String get callIncomingAudio => 'Chiamata audio';

  @override
  String get callAccept => 'Accetta';

  @override
  String get callDecline => 'Rifiuta';

  @override
  String get chatAudioNote => 'Nota audio';

  @override
  String get chatVideoNote => 'Video messaggio';

  @override
  String get chatSendNote => 'Invia nota';

  @override
  String get callNoPermission => 'Permesso microfono negato';

  @override
  String get callMute => 'Microfono';

  @override
  String get callUnmute => 'Riattiva microfono';

  @override
  String get callCamera => 'Fotocamera';

  @override
  String get callScreenShare => 'Condividi schermo';

  @override
  String get callEnd => 'Termina';

  @override
  String get callConnecting => 'Connessione…';

  @override
  String get callWaiting => 'In attesa…';

  @override
  String get callNoAnswer => 'Nessuna risposta';

  @override
  String get chatSearchAllTitle => 'Cerca nelle chat';

  @override
  String get chatSearchAllHint => 'Cerca in tutte le conversazioni';

  @override
  String get chatSearchTooltip => 'Cerca nei messaggi';

  @override
  String get chatSearchHint => 'Cerca nella chat';

  @override
  String get chatSearchNoResults => 'Nessun messaggio trovato';

  @override
  String get chatForward => 'Inoltra';

  @override
  String get chatForwardTitle => 'Inoltra a…';

  @override
  String get chatForwarded => 'Messaggio inoltrato';

  @override
  String get chatMediaFallback => 'Allegato';

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
  String get chatEdited => 'modificato';

  @override
  String get chatRecordingEmpty => 'Non è stato registrato nulla. Riprova.';

  @override
  String get chatStatusPending => 'In attesa';

  @override
  String get chatStatusQueued => 'In coda';

  @override
  String get chatStatusSent => 'Inviato';

  @override
  String get chatStatusDelivered => 'Consegnato';

  @override
  String get chatStatusSeenAll => 'Letto da tutti';

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
      'La chat verrà rimossa dal tuo dispositivo.';

  @override
  String get chatDeleteGroup => 'Elimina gruppo';

  @override
  String get chatDeleteGroupHint =>
      'Il gruppo verrà eliminato per tutti i membri.';

  @override
  String get chatLeaveGroup => 'Esci dal gruppo';

  @override
  String get chatLeaveGroupHint =>
      'Il gruppo resterà disponibile per gli altri membri.';

  @override
  String get chatMoreActions => 'Altre azioni';

  @override
  String get chatPin => 'Fissa chat';

  @override
  String get chatUnpin => 'Rimuovi fissata';

  @override
  String get chatPinMessage => 'Fissa messaggio';

  @override
  String get chatUnpinMessage => 'Rimuovi messaggio fissato';

  @override
  String get chatPinned => 'Messaggio fissato';

  @override
  String get chatPinnedMessage => 'MESSAGGIO FISSATO';

  @override
  String get chatUnpinned => 'Messaggio rimosso dai fissati';

  @override
  String get chatInviteLink => 'Copia link invito';

  @override
  String get chatInviteCopied => 'Link invito copiato';

  @override
  String get chatJoinTitle => 'Unisciti a un gruppo';

  @override
  String get chatJoinHint => 'Incolla il link invito (fedi3://chat/...)';

  @override
  String get chatJoinAction => 'Unisciti';

  @override
  String get chatJoinInvalid => 'Link invito non valido';

  @override
  String get chatJoinSuccess => 'Sei entrato nel gruppo';

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
      'Timeline Generica (P2P rete globale + Legacy). Mostra anche interazioni in ingresso/uscita con utenti che non segui e non ti seguono.';

  @override
  String get timelineLocalTooltip =>
      'Timeline Follower/Follow (P2P + Legacy). Solo profili con cui hai relazione follow/follower.';

  @override
  String get timelineSocialTooltip =>
      'Timeline Sociale (P2P). Contenuti best-effort diffusi via gossip/DHT (scoperta peer oltre i legacy).';

  @override
  String get timelineFederatedTooltip =>
      'Timeline Federata (P2P + Legacy). Mostra contenuti pubblici visti dalla tua istanza nella federazione.';

  @override
  String get timelineLayoutColumns => 'Passa alle colonne';

  @override
  String get timelineLayoutTabs => 'Passa alle tab';

  @override
  String get coreStart => 'Avvia core';

  @override
  String get coreStop => 'Ferma core';

  @override
  String get timelineEmptyHint =>
      'Segui persone o esplora il Fediverso per riempire questa timeline.';

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
  String get relayDirectoryTitle => 'Directory relay';

  @override
  String get relayDirectoryHint =>
      'Relay pubblicati in relays/relays.json, ordinati per la latenza misurata da questo dispositivo.';

  @override
  String get relayDirectoryRefresh => 'Aggiorna';

  @override
  String get relayDirectoryMeasuring => 'Misura della latenza…';

  @override
  String get relayDirectoryEmpty => 'Nessun relay disponibile';

  @override
  String relayDirectoryError(String error) {
    return 'Directory non disponibile: $error';
  }

  @override
  String get relayDirectoryAddYours => 'Aggiungi il tuo relay';

  @override
  String get relayDirectoryUse => 'Usa';

  @override
  String relayDirectoryRank(int rank) {
    return '#$rank';
  }

  @override
  String get relayDirectoryTapHint => 'Tocca un relay per usarlo.';

  @override
  String relayLatencyMs(int ms) {
    return '$ms ms';
  }

  @override
  String get relayLatencyOffline => 'non raggiungibile';

  @override
  String get relayLatencyTlsMismatch => 'host TLS non corrispondente';

  @override
  String get relayLatencyTlsCertificate => 'certificato TLS non valido';

  @override
  String get relayLatencyTimeout => 'timeout';

  @override
  String get relayLatencyHttpError => 'errore HTTP';

  @override
  String get relayLatencyNetworkError => 'errore di rete';

  @override
  String relayLatencyMeasuredAt(String time) {
    return 'Misurata alle $time';
  }

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
  String get onboardingAdvanced => 'Impostazioni avanzate';

  @override
  String get onboardingRecoveryTitle => 'Codice di recupero';

  @override
  String get onboardingInstanceUrl => 'URL istanza (https://...)';

  @override
  String get onboardingRecoveryCode => 'Codice di recupero (relay token)';

  @override
  String get onboardingConnect => 'Connetti';

  @override
  String get onboardingManualConfig => 'Configura manualmente';

  @override
  String get onboardingRecoveryHint =>
      'Conserva questi codici: ti servono per accedere alla tua identità su un altro dispositivo o dopo un ripristino.';

  @override
  String get onboardingCopy => 'Copia';

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
  String get backgroundServiceTitle => 'Notifiche in background';

  @override
  String get backgroundServiceHint =>
      'Mantiene Fedi3 attivo mentre l\'app è in background così le nuove attività ti raggiungono. Viene mostrata una piccola notifica persistente.';

  @override
  String get backgroundServiceRunning => 'Servizio attivo';

  @override
  String get backgroundServiceIdle => 'Servizio in pausa (app in primo piano)';

  @override
  String get backgroundServiceNotificationBody => 'Connesso per le notifiche';

  @override
  String get composeAddMediaTitle => 'Aggiungi media';

  @override
  String get composeAddMediaHint => 'Immagini, video o audio';

  @override
  String get composeDropMediaTitle => 'Trascina qui i tuoi media';

  @override
  String get composeDropMediaHint =>
      'Immagini, video o audio — oppure scegli i file';

  @override
  String get composeChooseFiles => 'Scegli file';

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
  String get composeQuickHint => 'Cosa succede nel Fediverso?';

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
  String get savedTitle => 'Salvati';

  @override
  String get savedHint => 'Disponibili anche offline';

  @override
  String get savedEmptyTitle => 'Nessun post salvato';

  @override
  String get savedEmptyHint =>
      'Tocca il segnalibro su un post per ritrovarlo qui.';

  @override
  String get savedRefresh => 'Aggiorna salvati';

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
  String get profileEditAction => 'Modifica profilo';

  @override
  String get profileLoadMore => 'Carica altri';

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
  String get telemetryEnabled => 'Diagnostica locale';

  @override
  String get telemetryEnabledHint =>
      'Salva errori e informazioni sulle prestazioni su questo dispositivo. Nessun invio automatico: puoi esportare il log manualmente.';

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
  String get networkingHintTitle => 'Relay, P2P e AP relays';

  @override
  String get networkingRelay => 'Relay public base URL';

  @override
  String get networkingRelayWs => 'Relay WebSocket';

  @override
  String get networkingBind => 'Bind locale';

  @override
  String get networkingP2p => 'P2P';

  @override
  String get networkingRelaysTitle => 'Discovery relay';

  @override
  String networkingRelaysCount(int count) {
    return '$count relay noti';
  }

  @override
  String get networkingRelaysEmpty => 'Nessun relay';

  @override
  String get networkingRelayDirectoryTitle => 'Directory relay condivisa';

  @override
  String get networkingRelayDirectoryHint =>
      'Ordinati per latenza misurata da questo dispositivo. Aggiorna per riprovare.';

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
  String get networkingAnchorTitle => 'Identità (anchor)';

  @override
  String get networkingAnchorLabel => 'URL anchor dell\'identità';

  @override
  String get networkingAnchorHint =>
      'Lascia vuoto per mantenere l\'identità sul relay. Imposta un dominio statico (es. https://alice.example) per separare l\'identità dal relay.';

  @override
  String get networkingAnchorInvalid =>
      'URL non valido: usa http:// o https:// senza slash finale.';

  @override
  String get networkingAnchorSave => 'Salva anchor';

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
  String backupErr(String error) {
    return 'Errore backup: $error';
  }

  @override
  String get statusRelay => 'Relay';

  @override
  String get statusP2p => 'P2P';

  @override
  String get statusMailbox => 'Mailbox';

  @override
  String get statusWebrtc => 'WebRTC';

  @override
  String get statusRelayRtt => 'R';

  @override
  String get statusP2pRtt => 'P';

  @override
  String get statusMailboxRtt => 'M';

  @override
  String get statusRelayTraffic => 'R ↑/↓';

  @override
  String get statusP2pTraffic => 'P ↑/↓';

  @override
  String get statusMailboxTraffic => 'M ↑/↓';

  @override
  String get statusWebrtcTraffic => 'W ↑/↓';

  @override
  String get statusCoreStoppedShort => 'core off';

  @override
  String get statusUnknownShort => '?';

  @override
  String get presenceChangeTooltip => 'Cambia stato';

  @override
  String get suggestedTitle => 'Account suggeriti';

  @override
  String get suggestedHint =>
      'Persone attive nel Fediverso che potresti seguire';

  @override
  String get trendingTitle => 'Scopri';

  @override
  String get trendingHint => 'Hashtag in tendenza nel Fediverso';

  @override
  String get presenceTitle => 'Stato';

  @override
  String get presenceHint => 'Scegli come apparire agli altri.';

  @override
  String get presenceOnline => 'Online';

  @override
  String get presenceBusy => 'Occupato';

  @override
  String get presenceAfk => 'AFK';

  @override
  String get presenceOffline => 'Offline';

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
  String get relaysOnlineUsers => 'Utenti online';

  @override
  String get relaysOnlinePeers => 'Peer online';

  @override
  String get relaysTotalUsers => 'Utenti conosciuti';

  @override
  String get relaysPeersSeen => 'Peer visti';

  @override
  String get relaysRelayCount => 'Relay nella mesh';

  @override
  String get relaysViewRaw => 'Mostra telemetria grezza';

  @override
  String get relaysTelemetry => 'Telemetria';

  @override
  String get relaysKnown => 'Relay conosciuti';

  @override
  String relaysLastSeenLabel(String time) {
    return 'Visto l\'ultima volta $time';
  }

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
  String get onboardingEnableP2p => 'Abilita P2P';

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
      'Relay public URL (https://relay.example.com)';

  @override
  String get editAccountRelayWs => 'Relay WS (wss://relay.example.com)';

  @override
  String get editAccountRegenerateInternal => 'Rigenera internal token';

  @override
  String get devCoreTitle => 'Controlli core (dev)';

  @override
  String get devCoreConfigSaved => 'Config (salvata)';

  @override
  String get devCoreStart => 'Avvia';

  @override
  String get devCoreStop => 'Ferma';

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
  String get notificationsToday => 'Oggi';

  @override
  String get notificationsYesterday => 'Ieri';

  @override
  String get notificationsEarlier => 'Precedenti';

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
  String get composerBlockStyle => 'Stile blocco';

  @override
  String get composerParagraph => 'Paragrafo';

  @override
  String get composerH1 => 'Titolo 1';

  @override
  String get composerH2 => 'Titolo 2';

  @override
  String get composerH3 => 'Titolo 3';

  @override
  String get composerQuote => 'Citazione';

  @override
  String get composerFont => 'Carattere';

  @override
  String get composerSans => 'Sans serif';

  @override
  String get composerSerif => 'Serif';

  @override
  String get composerMono => 'Monospazio';

  @override
  String get composerSize => 'Dimensione';

  @override
  String get composerSizeS => 'Piccolo';

  @override
  String get composerSizeL => 'Grande';

  @override
  String get composerSizeXL => 'Molto grande';

  @override
  String get composerAlignLeft => 'Allinea a sinistra';

  @override
  String get composerAlignCenter => 'Centra';

  @override
  String get composerAlignRight => 'Allinea a destra';

  @override
  String get composerUndo => 'Annulla';

  @override
  String get composerRedo => 'Ripeti';

  @override
  String get composerEdit => 'Modifica';

  @override
  String get composerPreview => 'Anteprima';

  @override
  String get composerBold => 'Grassetto';

  @override
  String get composerItalic => 'Corsivo';

  @override
  String get composerStrike => 'Barrato';

  @override
  String get composerCode => 'Codice';

  @override
  String get composerLink => 'Link';

  @override
  String get composerBulletList => 'Elenco puntato';

  @override
  String get composerNumberedList => 'Elenco numerato';

  @override
  String get composerDivider => 'Separatore';

  @override
  String get composerEmoji => 'Emoji';

  @override
  String composerStats(int words, int chars, int minutes) {
    return '$words parole · $chars caratteri · $minutes min di lettura';
  }

  @override
  String get composerPreviewMode => 'Anteprima live';

  @override
  String get composerPreviewEmpty => 'Scrivi qualcosa per vedere l’anteprima.';

  @override
  String get composerInsertLink => 'Inserisci link';

  @override
  String get composerLinkUrl => 'URL';

  @override
  String get composerInsert => 'Inserisci';

  @override
  String get composeMfmCheatsheetTitle => 'MFM cheatsheet';

  @override
  String get composeMfmCheatsheetBody =>
      '**grassetto** -> grassetto\n*corsivo* -> corsivo\n~~barrato~~ -> barrato\n`codice` -> codice in linea\n#tag -> hashtag\n:emoji: -> emoji personalizzata\nA capo: nuova linea';

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
  String get composeExpand => 'Scrivi un post';

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
  String get p2pRelayReserveLabel =>
      'P2P relay reserve (multiaddr, una per riga)';

  @override
  String get postDeliveryModeLabel => 'Modalità consegna post';

  @override
  String get postDeliveryP2pRelay => 'P2P + Relay (fallback dopo 5s)';

  @override
  String get postDeliveryP2pOnly => 'Solo P2P (mai relay)';

  @override
  String get webrtcSectionTitle => 'WebRTC (ICE/TURN)';

  @override
  String get webrtcSectionHint =>
      'Usa TURN/STUN come fallback P2P (consigliato)';

  @override
  String get webrtcAutofill => 'Auto-popola ICE da relay';

  @override
  String get webrtcIceUrlsLabel => 'WebRTC ICE URLs (una per riga)';

  @override
  String get webrtcIceUsernameLabel => 'WebRTC ICE username';

  @override
  String get webrtcIceCredentialLabel => 'WebRTC ICE credential';

  @override
  String get fileTypeJson => 'JSON';

  @override
  String get notificationsMarkAllRead => 'Segna tutto come letto';

  @override
  String get notificationsFilterAll => 'Tutte';

  @override
  String get notificationsFilterMentions => 'Menzioni';

  @override
  String get notificationsFilterReactions => 'Reazioni';

  @override
  String get notificationsFilterFollows => 'Follow';

  @override
  String get notificationsFilterBoosts => 'Boost';

  @override
  String get notificationsFilterLikes => 'Like';

  @override
  String get notificationsFollowedBack => 'Follow restituito';

  @override
  String get notificationsFollowBack => 'Segui anche tu';

  @override
  String notificationsFollowBackFailed(String error) {
    return 'Follow restituito fallito: $error';
  }

  @override
  String get notificationsReply => 'Rispondi';

  @override
  String get timelineFiltersTooltip => 'Filtri';

  @override
  String get timelineFiltersTitle => 'Filtri';

  @override
  String get timelineFilterMedia => 'Media';

  @override
  String get timelineFilterReply => 'Risposte';

  @override
  String get timelineFilterBoost => 'Boost';

  @override
  String get timelineFilterMention => 'Menzioni';

  @override
  String get mediaDownload => 'Scarica';

  @override
  String get mediaShare => 'Condividi';

  @override
  String get mediaAutoplayOn => 'Riproduzione automatica attiva';

  @override
  String get mediaAutoplayOff => 'Riproduzione automatica disattivata';

  @override
  String get mediaMuted => 'Muto';

  @override
  String get mediaSoundOn => 'Audio attivo';

  @override
  String get mediaOpenInBrowser => 'Apri nel browser';

  @override
  String get mediaDownloaded => 'Scaricato';

  @override
  String mediaDownloadFailed(String error) {
    return 'Download fallito: $error';
  }

  @override
  String get noteThreadMuted => 'Thread silenziato';

  @override
  String get noteUnmute => 'Riattiva audio';

  @override
  String get activityNotificationTitle => 'Nuova attività';

  @override
  String get activityNotificationBody => 'Hai nuova attività su Fedi3.';

  @override
  String get noteMoreActions => 'Altre azioni';

  @override
  String get noteBookmark => 'Salva';

  @override
  String get noteUnbookmark => 'Rimuovi dai salvati';

  @override
  String get notePin => 'Fissa';

  @override
  String get noteUnpin => 'Rimuovi fissaggio';

  @override
  String get noteMute => 'Silenzia thread';

  @override
  String get noteLinkCopied => 'Link copiato';

  @override
  String get noteUsers => 'Utenti';

  @override
  String noteReactionsTitle(String label) {
    return 'Reazioni $label';
  }

  @override
  String get semanticsAvatar => 'Avatar';

  @override
  String get semanticsMedia => 'Allegato media';

  @override
  String get onboardingErrUsername =>
      'Lo username deve avere 1-64 caratteri: lettere, cifre o underscore.';

  @override
  String get onboardingErrDomain =>
      'Inserisci un dominio valido (es. example.com).';

  @override
  String get onboardingErrPublicUrl =>
      'L\'URL pubblico deve essere un URL http(s) assoluto.';

  @override
  String get onboardingErrRelayWs =>
      'L\'URL WebSocket del relay deve iniziare con ws:// o wss://.';

  @override
  String get onboardingErrBind =>
      'L\'indirizzo di bind deve essere host:porta (es. 127.0.0.1:8788).';

  @override
  String get settingsFollowRequests => 'Richieste di follow';

  @override
  String get settingsFollowRequestsHint =>
      'Approva o rifiuta chi vuole seguire questo account bloccato.';

  @override
  String get followRequestsTitle => 'Richieste di follow';

  @override
  String get followRequestsEmpty => 'Nessuna richiesta in attesa.';

  @override
  String get followRequestsApprove => 'Approva';

  @override
  String get followRequestsReject => 'Rifiuta';

  @override
  String followRequestsApproved(String handle) {
    return 'Approvato $handle';
  }

  @override
  String followRequestsRejected(String handle) {
    return 'Rifiutato $handle';
  }

  @override
  String followRequestsLoadFailed(String error) {
    return 'Impossibile caricare le richieste di follow: $error';
  }

  @override
  String get followRequestsCoreStopped =>
      'Avvia il core per vedere le richieste di follow.';

  @override
  String get networkingDiscoveryTitle => 'Scoperta dei peer';

  @override
  String get networkingPublicDiscovery =>
      'Usa la rete pubblica IPFS per trovare peer';

  @override
  String get networkingPublicDiscoveryHint =>
      'Bootstrap e ricerca aggiuntivi. I messaggi Fedi3 non vengono salvati in IPFS.';

  @override
  String get networkingPeerExchange =>
      'Ricorda i peer e scambia contatti firmati';

  @override
  String get networkingPeerExchangeHint =>
      'Ricollega i peer conosciuti e aiuta la rete Fedi3 a crescere. Gli annunci pubblici espongono gli indirizzi di rete.';

  @override
  String get networkingBootstrapLabel => 'Nodi bootstrap Fedi3 indipendenti';

  @override
  String get networkingBootstrapHint =>
      'Un multiindirizzo libp2p per riga. Le modifiche riavviano il core locale.';

  @override
  String get profileImagesTitle => 'Immagini profilo';

  @override
  String get profileImageDropHint =>
      'Trascina qui un\'immagine o clicca per sceglierla';

  @override
  String get profilePasteImage => 'Incolla immagine';

  @override
  String get profileRemoveImage => 'Rimuovi immagine';

  @override
  String get profileDetailsTitle => 'Dettagli profilo';

  @override
  String get backupScopeWarning =>
      'Backup cifrato delle impostazioni: non include identità crittografica, messaggi o media. Per trasferire la stessa identità occorre anche una copia protetta della cartella dati del core. Conserva la passphrase separatamente.';

  @override
  String get backupPassphrase => 'Passphrase (almeno 12 caratteri)';

  @override
  String get configRecoveryTitle => 'Configurazione non disponibile';

  @override
  String get keyringUnavailableHint =>
      'Sblocca il portachiavi di sistema e premi Riprova. Su Linux serve un servizio Secret Service attivo (GNOME Keyring o KWallet) e libsecret. La configurazione esistente è conservata.';

  @override
  String get corruptConfigHint =>
      'Impossibile leggere la configurazione e la sua copia di sicurezza. Ripristina una copia valida del file, poi premi Riprova. I dati esistenti sono conservati.';

  @override
  String get clientUpdatesTitle => 'Aggiornamenti del client';

  @override
  String get clientUpdatesHint =>
      'Controlla le release ufficiali. I download vengono verificati prima dell’installazione.';

  @override
  String get clientUpdatesCheck => 'Controlla aggiornamenti';

  @override
  String clientUpdatesAvailable(String version) {
    return 'È disponibile la versione $version';
  }

  @override
  String clientUpdatesVersion(String version) {
    return 'Versione installata: $version';
  }

  @override
  String get clientUpdatesCurrent =>
      'Non sono disponibili release stabili più recenti.';

  @override
  String get clientUpdatesRelease => 'Apri release';

  @override
  String get clientUpdatesDownload => 'Scarica aggiornamento verificato';

  @override
  String get clientUpdatesVerified => 'Download completato. SHA256 verificato.';

  @override
  String get clientUpdatesManual =>
      'Chiudi Fedi3 prima di sostituire l’applicazione installata con il nuovo pacchetto. Conserva i dati del profilo. Su mobile, usa la fonte di installazione o lo store.';

  @override
  String get clientUpdatesInstall => 'Installa aggiornamento AppImage';

  @override
  String get clientUpdatesFolder => 'Apri cartella download';

  @override
  String get clientUpdatesRestart =>
      'Aggiornamento installato. Chiudi e riapri Fedi3 per usarlo. La precedente AppImage è salvata con il suffisso .previous.';

  @override
  String get chatGroupCreated => 'Gruppo creato';

  @override
  String get chatGroupRenamed => 'Gruppo rinominato';

  @override
  String get chatMemberAdded => 'Membro aggiunto';

  @override
  String get chatMemberRemoved => 'Membro rimosso';

  @override
  String get chatMemberLeft => 'Membro uscito';

  @override
  String get chatThreadDeleted => 'Chat eliminata';

  @override
  String chatSendFailed(String error) {
    return 'Invio non riuscito: $error';
  }

  @override
  String get retry => 'Riprova';
}
