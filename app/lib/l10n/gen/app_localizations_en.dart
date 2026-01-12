// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for English (`en`).
class AppLocalizationsEn extends AppLocalizations {
  AppLocalizationsEn([String locale = 'en']) : super(locale);

  @override
  String get appTitle => 'Fedi3';

  @override
  String get navTimeline => 'Timeline';

  @override
  String get navCompose => 'Compose';

  @override
  String get navNotifications => 'Notifications';

  @override
  String get navSearch => 'Search';

  @override
  String get navChat => 'Chat';

  @override
  String get navRelays => 'Relays';

  @override
  String get navSettings => 'Settings';

  @override
  String get save => 'Save';

  @override
  String get cancel => 'Cancel';

  @override
  String get ok => 'OK';

  @override
  String err(String error) {
    return 'Error: $error';
  }

  @override
  String get copy => 'Copy';

  @override
  String get copied => 'Copied';

  @override
  String get enabled => 'Enabled';

  @override
  String get disabled => 'Disabled';

  @override
  String get networkErrorTitle => 'Network issue';

  @override
  String get networkErrorHint => 'Check your connection and try again.';

  @override
  String get networkErrorRetry => 'Retry';

  @override
  String get timelineTitle => 'Timeline';

  @override
  String get timelineTabHome => 'Home';

  @override
  String get timelineTabLocal => 'Local';

  @override
  String get timelineTabSocial => 'Social';

  @override
  String get timelineTabFederated => 'Federated';

  @override
  String get searchTitle => 'Search';

  @override
  String get searchHint => 'Search posts, users, hashtags';

  @override
  String get searchTabPosts => 'Posts';

  @override
  String get searchTabUsers => 'Users';

  @override
  String get searchTabHashtags => 'Hashtags';

  @override
  String get searchEmpty => 'Type to search';

  @override
  String get searchNoResults => 'No results';

  @override
  String get searchShowingFor => 'Showing results for';

  @override
  String get searchSourceAll => 'All';

  @override
  String get searchSourceLocal => 'Local';

  @override
  String get searchSourceRelay => 'Relay';

  @override
  String searchTagCount(int count) {
    return '$count posts';
  }

  @override
  String get chatTitle => 'Chat';

  @override
  String get chatThreadTitle => 'Conversation';

  @override
  String get chatNewTitle => 'New chat';

  @override
  String get chatNewTooltip => 'Start a new chat';

  @override
  String get chatNewMissingFields => 'Add recipients and a message.';

  @override
  String get chatNewFailed => 'Failed to create chat.';

  @override
  String get chatRecipients => 'Recipients';

  @override
  String get chatRecipientsHint =>
      'e.g. @alice@example.com, https://server/users/bob';

  @override
  String get chatMessage => 'Message';

  @override
  String get chatMessageHint => 'Write a message…';

  @override
  String get chatCreate => 'Create';

  @override
  String get chatSend => 'Send';

  @override
  String get chatRefresh => 'Refresh';

  @override
  String get chatThreadsEmpty => 'No chats yet';

  @override
  String get chatEmpty => 'No messages yet';

  @override
  String get chatNoMessages => 'No messages';

  @override
  String get chatDirectMessage => 'Direct message';

  @override
  String get chatGroup => 'Group chat';

  @override
  String get chatMessageDeleted => 'Message deleted';

  @override
  String get chatMessageEmpty => 'Empty message';

  @override
  String get chatEdit => 'Edit';

  @override
  String get chatDelete => 'Delete';

  @override
  String get chatEditTitle => 'Edit message';

  @override
  String get chatDeleteTitle => 'Delete message';

  @override
  String get chatDeleteHint => 'This will remove the message for everyone.';

  @override
  String get chatSave => 'Save';

  @override
  String get chatNewMessage => 'New message';

  @override
  String get chatNewMessageBody => 'You have a new message.';

  @override
  String get chatOpen => 'Open';

  @override
  String get chatGif => 'GIF';

  @override
  String get chatGifSearchHint => 'Search GIFs…';

  @override
  String get chatGifEmpty => 'No GIFs found';

  @override
  String get chatGifMissingKey =>
      'Add a GIF API key in Settings to enable search.';

  @override
  String get chatStatusPending => 'Pending';

  @override
  String get chatStatusQueued => 'Queued';

  @override
  String get chatStatusSent => 'Sent';

  @override
  String get chatStatusDelivered => 'Delivered';

  @override
  String get chatStatusSeen => 'Seen';

  @override
  String get chatReply => 'Reply';

  @override
  String get chatReplyClear => 'Cancel reply';

  @override
  String get chatReplyAttachment => 'Attachment';

  @override
  String get chatReplyUnknown => 'Original message';

  @override
  String get chatMembers => 'Members';

  @override
  String get chatRename => 'Rename chat';

  @override
  String get chatRenameHint => 'New chat title';

  @override
  String get chatAddMember => 'Add member';

  @override
  String get chatAddMemberHint => 'Search or paste an actor';

  @override
  String get chatRemoveMember => 'Remove member';

  @override
  String get chatReact => 'React';

  @override
  String get chatPin => 'Pin chat';

  @override
  String get chatUnpin => 'Unpin chat';

  @override
  String chatTyping(String name) {
    return '$name is typing…';
  }

  @override
  String chatTypingMany(String names) {
    return '$names are typing…';
  }

  @override
  String get timelineHomeTooltip =>
      'Generic timeline (Global P2P + Legacy). Shows incoming/outgoing interactions, including with users you don\'t follow and who don\'t follow you.';

  @override
  String get timelineLocalTooltip =>
      'Follower/Following timeline (P2P + Legacy). Only profiles with a follow/follower relationship.';

  @override
  String get timelineSocialTooltip =>
      'Social timeline (P2P). Best-effort global feed via gossip/DHT (discover peers beyond legacy).';

  @override
  String get timelineFederatedTooltip =>
      'Federated timeline (P2P + Legacy). Public content seen by your instance across federation.';

  @override
  String get timelineLayoutColumns => 'Switch to columns';

  @override
  String get timelineLayoutTabs => 'Switch to tabs';

  @override
  String get coreStart => 'Start core';

  @override
  String get coreStop => 'Stop core';

  @override
  String get listNoItems => 'No items';

  @override
  String get listEnd => 'End';

  @override
  String get listLoadMore => 'Load more';

  @override
  String timelineNewPosts(int count) {
    return '$count new posts';
  }

  @override
  String get timelineShowNewPosts => 'Show';

  @override
  String get timelineShowNewPostsHint => 'Click to show without reloading';

  @override
  String get timelineRefreshTitle => 'Refresh';

  @override
  String get timelineRefreshHint =>
      'Force reload and pull from legacy while you were offline';

  @override
  String get activityBoost => 'Boost';

  @override
  String get activityReply => 'Reply';

  @override
  String get activityLike => 'Like';

  @override
  String get activityReact => 'React';

  @override
  String get activityReplyTitle => 'Reply';

  @override
  String get activityReplyHint => 'Write a reply.';

  @override
  String get activityCancel => 'Cancel';

  @override
  String get activitySend => 'Send';

  @override
  String get activityUnsupported => 'Unsupported item';

  @override
  String get activityViewRaw => 'View raw';

  @override
  String get activityHideRaw => 'Hide raw';

  @override
  String get noteInReplyTo => 'In reply to';

  @override
  String noteBoostedBy(String name) {
    return 'Boosted by $name';
  }

  @override
  String get noteThreadTitle => 'Thread';

  @override
  String get noteThreadUp => 'In reply to';

  @override
  String get noteShowThread => 'Show thread';

  @override
  String get noteHideThread => 'Hide thread';

  @override
  String get noteLoadingThread => 'Loading…';

  @override
  String get noteShowReplies => 'Show replies';

  @override
  String get noteHideReplies => 'Hide replies';

  @override
  String get noteLoadingReplies => 'Loading replies…';

  @override
  String get noteReactionLoading => 'Loading reactions…';

  @override
  String get noteReactionAdd => 'Add reaction';

  @override
  String get noteContentWarning => 'Content warning';

  @override
  String get noteShowContent => 'Show';

  @override
  String get noteHideContent => 'Hide';

  @override
  String get timeAgoJustNow => 'just now';

  @override
  String timeAgoMinutes(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count minutes ago',
      one: '1 minute ago',
    );
    return '$_temp0';
  }

  @override
  String timeAgoHours(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count hours ago',
      one: '1 hour ago',
    );
    return '$_temp0';
  }

  @override
  String timeAgoDays(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count days ago',
      one: '1 day ago',
    );
    return '$_temp0';
  }

  @override
  String timeAgoMonths(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count months ago',
      one: '1 month ago',
    );
    return '$_temp0';
  }

  @override
  String timeAgoYears(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count years ago',
      one: '1 year ago',
    );
    return '$_temp0';
  }

  @override
  String get firstRunTitle => 'Welcome';

  @override
  String get firstRunIntro =>
      'Do you want to use an existing account or create a new one?';

  @override
  String get firstRunLoginTitle => 'Log in (existing setup)';

  @override
  String get firstRunLoginHint =>
      'Enter the settings of an existing account/setup.';

  @override
  String get firstRunCreateTitle => 'Create new account';

  @override
  String get firstRunCreateHint =>
      'Create a new local identity and configure relay endpoints.';

  @override
  String get firstRunRelayPreviewTitle => 'Relay network preview';

  @override
  String get firstRunRelayPreviewRelays => 'Relays';

  @override
  String get firstRunRelayPreviewPeers => 'Peers';

  @override
  String get firstRunRelayPreviewEmpty => 'No data yet';

  @override
  String get firstRunRelayPreviewError => 'Failed to load relay preview.';

  @override
  String get onboardingCreateTitle => 'Create account';

  @override
  String get onboardingCreateIntro =>
      'Fill the required fields to create a new setup. The core will start automatically after saving.';

  @override
  String get onboardingLoginTitle => 'Log in';

  @override
  String get onboardingLoginIntro =>
      'Enter your existing setup fields. The core will start automatically after saving.';

  @override
  String get onboardingImportBackup => 'Import backup JSON';

  @override
  String get onboardingRelaySelect => 'Relay selection';

  @override
  String get onboardingRelayDiscover => 'Discover';

  @override
  String get onboardingRelayDiscovering => 'Discovering…';

  @override
  String get onboardingRelayListHint =>
      'Pick a relay from the shared list or enter one manually.';

  @override
  String get onboardingRelayPick => 'Select a relay';

  @override
  String get onboardingRelayCustom => 'Use custom relay settings';

  @override
  String get composeTitle => 'Compose';

  @override
  String composeCharCount(int used, int max) {
    return '$used/$max';
  }

  @override
  String composeCoreNotRunning(String settings, String start) {
    return 'Core not running: go to $settings and press $start.';
  }

  @override
  String get composeWhatsHappening => 'Write what you think.';

  @override
  String get composePublic => 'Public';

  @override
  String get composePublicHint => 'If off: followers-only';

  @override
  String get composeAddMediaPath => 'Add media (path)';

  @override
  String get composeAddMedia => 'Attach media';

  @override
  String get composeDropHere => 'Drop files to attach';

  @override
  String get composeClearDraft => 'Clear draft';

  @override
  String get composeDraftSaved => 'Draft saved';

  @override
  String get composeDraftRestored => 'Draft restored';

  @override
  String get composeDraftCleared => 'Draft cleared';

  @override
  String get composeDraftResumeTooltip => 'Resume draft';

  @override
  String get composeDraftDeleteTooltip => 'Delete draft';

  @override
  String get composeQuickHint => 'Open the quick composer';

  @override
  String get translationTitle => 'Translation';

  @override
  String get translationHint => 'DeepL or DeepLX settings';

  @override
  String get translationProviderLabel => 'Provider';

  @override
  String get translationProviderDeepL => 'DeepL';

  @override
  String get translationProviderDeepLX => 'DeepLX';

  @override
  String get translationAuthKeyLabel => 'Auth key';

  @override
  String get translationAuthKeyHint => 'DeepL API key';

  @override
  String get translationUseProLabel => 'Pro account';

  @override
  String get translationUseProHint =>
      'Use api.deepl.com instead of api-free.deepl.com';

  @override
  String get translationDeepLXUrlLabel => 'DeepLX endpoint';

  @override
  String get translationDeepLXUrlHint => 'https://api.deeplx.org/translate';

  @override
  String get translationTimeoutLabel => 'Timeout';

  @override
  String translationTimeoutValue(int seconds) {
    String _temp0 = intl.Intl.pluralLogic(
      seconds,
      locale: localeName,
      other: '# seconds',
      one: '# second',
    );
    return '$_temp0';
  }

  @override
  String translationTargetLabel(String lang) {
    return 'Target language: $lang';
  }

  @override
  String get noteTranslate => 'Translate';

  @override
  String get noteShowTranslation => 'Show translation';

  @override
  String get noteShowOriginal => 'Show original';

  @override
  String noteTranslatedFrom(String lang) {
    return 'Translated from $lang';
  }

  @override
  String noteTranslateFailed(String error) {
    return 'Translation failed: $error';
  }

  @override
  String get composePost => 'Post';

  @override
  String get composeAttachments => 'Attachments';

  @override
  String composeMediaId(String id) {
    return 'mediaId=$id';
  }

  @override
  String get composeFileFallback => 'file';

  @override
  String get composeMediaFilePathLabel => 'Media file path (desktop/dev)';

  @override
  String get composeMediaFilePathHint =>
      'C:\\\\path\\\\img.png or /home/user/img.png';

  @override
  String get composeNotUploaded => 'Not uploaded';

  @override
  String get composeQueuedOk => 'OK: queued for delivery';

  @override
  String get composeErrEmptyContent => 'ERR: empty content';

  @override
  String composeErrUnableReadFile(String error) {
    return 'ERR: unable to read file: $error';
  }

  @override
  String composeErrUnablePickFile(String error) {
    return 'ERR: file picker failed: $error';
  }

  @override
  String composeErrInvalidMediaType(String type) {
    return 'ERR: invalid media type: $type';
  }

  @override
  String composeErrGeneric(String error) {
    return 'ERR: $error';
  }

  @override
  String get settingsTitle => 'Settings';

  @override
  String get settingsCore => 'Core';

  @override
  String settingsCoreRunning(int handle) {
    return 'Running (handle=$handle)';
  }

  @override
  String get settingsCoreStopped => 'Stopped';

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
  String get settingsAdvancedDev => 'Advanced (dev)';

  @override
  String get settingsAdvancedDevHint => 'Start/Stop + migration status';

  @override
  String get settingsResetApp => 'Reset app (clear config)';

  @override
  String get profileEditTitle => 'Profile';

  @override
  String get profileEditHint => 'Public profile, avatar, banner, fields';

  @override
  String get profileDisplayName => 'Display name';

  @override
  String get profileBio => 'Bio';

  @override
  String get profileFollowers => 'Followers';

  @override
  String get profileFollowing => 'Following';

  @override
  String get profileAvatar => 'Avatar';

  @override
  String get profileBanner => 'Banner';

  @override
  String get profilePickFile => 'Choose file';

  @override
  String get profileFilePathHint => 'File path (desktop)';

  @override
  String get profileUpload => 'Upload';

  @override
  String get profileUploadOk => 'OK: uploaded';

  @override
  String get profileSave => 'Save';

  @override
  String get profileSavedOk => 'OK: saved (core restarted)';

  @override
  String profileErrSave(String error) {
    return 'Save failed: $error';
  }

  @override
  String profileErrUpload(String error) {
    return 'Upload failed: $error';
  }

  @override
  String get profileErrCoreNotRunning => 'Core not running.';

  @override
  String get profileFieldsTitle => 'Profile fields';

  @override
  String get profileFieldAdd => 'Add field';

  @override
  String get profileFieldEdit => 'Edit field';

  @override
  String get profileFieldName => 'Name';

  @override
  String get profileFieldValue => 'Value';

  @override
  String get profileFieldNameEmpty => '(empty)';

  @override
  String get privacyTitle => 'Privacy';

  @override
  String get privacyHint => 'Account privacy settings';

  @override
  String get privacyLockedAccount => 'Locked account (manual follow approval)';

  @override
  String get privacyLockedAccountHint =>
      'When enabled, incoming follow requests require approval.';

  @override
  String get telemetrySectionTitle => 'Diagnostics';

  @override
  String get telemetryEnabled => 'Anonymous telemetry';

  @override
  String get telemetryEnabledHint =>
      'Share minimal diagnostics (errors and performance) to improve stability. No personal content.';

  @override
  String get telemetryMonitoringEnabled => 'Client monitoring (debug/staging)';

  @override
  String get telemetryMonitoringHint =>
      'Keep a local diagnostics log for troubleshooting.';

  @override
  String get telemetryOpen => 'Open diagnostics';

  @override
  String get telemetryTitle => 'Diagnostics';

  @override
  String get telemetryRefresh => 'Refresh';

  @override
  String get telemetryExport => 'Export';

  @override
  String get telemetryExportEmpty => 'No diagnostics to export';

  @override
  String get telemetryClear => 'Clear';

  @override
  String get telemetryEmpty => 'No diagnostics yet';

  @override
  String get securityTitle => 'Security';

  @override
  String get securityHint => 'Tokens and internal endpoints';

  @override
  String get securityInternalToken => 'Internal token';

  @override
  String get securityInternalTokenHint =>
      'Used to protect internal API endpoints';

  @override
  String get securityRegenerate => 'Regenerate';

  @override
  String get securityHintInternalEndpoints =>
      'Keep this token private. If exposed, others on your network could call internal endpoints.';

  @override
  String get moderationTitle => 'Moderation';

  @override
  String get moderationHintTitle => 'Block lists and policies';

  @override
  String get moderationBlockedDomains => 'Blocked domains';

  @override
  String get moderationBlockedDomainsHint =>
      'One per line (example.com or *.example.com)';

  @override
  String get moderationBlockedActors => 'Blocked actors';

  @override
  String get moderationBlockedActorsHint => 'One actor URL per line';

  @override
  String get moderationHint =>
      'Blocks apply to inbound and outbound interactions.';

  @override
  String get networkingTitle => 'Networking';

  @override
  String get networkingHintTitle => 'Relay, P2P, and AP relays';

  @override
  String get networkingRelay => 'Relay public base URL';

  @override
  String get networkingRelayWs => 'Relay WebSocket';

  @override
  String get networkingBind => 'Local bind';

  @override
  String get networkingP2p => 'P2P';

  @override
  String get networkingRelaysTitle => 'Relay discovery';

  @override
  String networkingRelaysCount(int count) {
    return '$count relays known';
  }

  @override
  String get networkingRelaysEmpty => 'No relays yet';

  @override
  String get relayAdminTitle => 'Relay admin';

  @override
  String get relayAdminHint => 'Manage relay users and audit';

  @override
  String get relayAdminRelayWsLabel => 'Relay WS';

  @override
  String get relayAdminTokenLabel => 'Admin token';

  @override
  String get relayAdminTokenHint => 'Paste admin token';

  @override
  String get relayAdminTokenMissing => 'Add an admin token to use relay admin.';

  @override
  String get relayAdminUsers => 'Users';

  @override
  String get relayAdminAudit => 'Audit';

  @override
  String get relayAdminRegister => 'Register';

  @override
  String get relayAdminRegisterHint => 'username (e.g. alice)';

  @override
  String get relayAdminUsername => 'Username';

  @override
  String get relayAdminGenerateToken => 'Generate token';

  @override
  String get relayAdminRotate => 'Rotate token';

  @override
  String get relayAdminEnable => 'Enable';

  @override
  String get relayAdminDisable => 'Disable';

  @override
  String get relayAdminDelete => 'Delete';

  @override
  String get relayAdminAuditFailed => 'Failed';

  @override
  String get relayAdminUserEnabled => 'Enabled';

  @override
  String get relayAdminUserDisabled => 'Disabled';

  @override
  String get relayAdminUserSearchHint => 'Search users…';

  @override
  String relayAdminDeleteConfirm(String username) {
    return 'Delete user $username?';
  }

  @override
  String get relayAdminAuditExport => 'Export audit';

  @override
  String relayAdminAuditExported(String path) {
    return 'Audit exported: $path';
  }

  @override
  String get relayAdminAuditSearchHint => 'Search audit…';

  @override
  String get relayAdminAuditFailedOnly => 'Failed only';

  @override
  String get relayAdminAuditReverse => 'Reverse order';

  @override
  String relayAdminUsersCount(int count) {
    return 'Users: $count';
  }

  @override
  String relayAdminUsersDisabledCount(int count) {
    return 'Disabled: $count';
  }

  @override
  String get relayAdminAuditLast => 'Last audit';

  @override
  String get networkingRelaysError => 'Relay sync failed';

  @override
  String get networkingApRelays => 'ActivityPub relays';

  @override
  String get networkingApRelaysEmpty => 'No AP relays configured';

  @override
  String get networkingEditAccount => 'Edit account/networking';

  @override
  String get networkingHint => 'Some changes require restarting the core.';

  @override
  String get backupTitle => 'Backup';

  @override
  String get backupHint => 'Export/import your Fedi3 profile and settings';

  @override
  String get backupExportTitle => 'Export';

  @override
  String get backupExportHint => 'Creates a JSON backup (config + UI prefs).';

  @override
  String get backupExportSave => 'Save backup file';

  @override
  String get backupExportCopy => 'Copy backup JSON';

  @override
  String get backupExportOk => 'OK: copied to clipboard';

  @override
  String get backupExportSaved => 'OK: saved backup file';

  @override
  String get backupImportTitle => 'Import';

  @override
  String get backupImportHint =>
      'Paste a previously exported JSON backup here.';

  @override
  String get backupImportApply => 'Import now';

  @override
  String get backupImportFile => 'Import from file';

  @override
  String get backupImportOk => 'OK: imported (core restarted)';

  @override
  String backupErr(String error) {
    return 'Backup error: $error';
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
  String get relaysTitle => 'Relays';

  @override
  String get relaysCurrent => 'Current relay';

  @override
  String get relaysTelemetry => 'Telemetry';

  @override
  String get relaysKnown => 'Known relays';

  @override
  String relaysLastSeen(String ms) {
    return 'last_seen=$ms';
  }

  @override
  String get relaysPeersTitle => 'Known peers';

  @override
  String get relaysPeersSearchHint => 'Search peers';

  @override
  String get relaysPeersEmpty => 'No peers found';

  @override
  String get relaysRecommended => 'Recommended';

  @override
  String relaysLatency(int ms) {
    return 'Latency: $ms ms';
  }

  @override
  String get relaysCoverageTitle => 'Search coverage';

  @override
  String relaysCoverageUsers(int indexed, int total) {
    return '$indexed/$total users indexed';
  }

  @override
  String relaysCoverageLast(String ms) {
    return 'Last index: $ms';
  }

  @override
  String get onboardingTitle => 'Fedi3 setup';

  @override
  String get onboardingIntro =>
      'Create your local instance and connect it to a relay (for legacy compatibility).';

  @override
  String get onboardingUsername => 'Username';

  @override
  String get onboardingDomain => 'Domain (handle: user@domain)';

  @override
  String get onboardingRelayPublicUrl =>
      'Relay public URL (https://relay... or http://127.0.0.1:8787)';

  @override
  String get onboardingRelayWs => 'Relay WS (wss://... or ws://127.0.0.1:8787)';

  @override
  String get onboardingRelayToken => 'Relay token';

  @override
  String get onboardingBind => 'Local bind (host:port)';

  @override
  String get onboardingInternalToken => 'Internal token (UI ↔ core)';

  @override
  String get onboardingEnableP2p => 'Enable P2P';

  @override
  String get onboardingSave => 'Save & open app';

  @override
  String get onboardingRelayTokenTooShort =>
      'Relay token too short (min 16 characters).';

  @override
  String get editAccountTitle => 'Edit account';

  @override
  String get editAccountSave => 'Save';

  @override
  String get editAccountCoreRunningWarning =>
      'Core is running: it will be stopped automatically before saving to avoid inconsistencies.';

  @override
  String get editAccountRelayPublicUrl =>
      'Relay public URL (https://relay.fedi3.com)';

  @override
  String get editAccountRelayWs => 'Relay WS (wss://relay.fedi3.com)';

  @override
  String get editAccountRegenerateInternal => 'Regenerate internal token';

  @override
  String get devCoreTitle => 'Dev core controls';

  @override
  String get devCoreConfigSaved => 'Config (saved)';

  @override
  String get devCoreStart => 'Start';

  @override
  String get devCoreStop => 'Stop';

  @override
  String get devCoreFetchMigration => 'Fetch migration status';

  @override
  String devCoreVersion(String version) {
    return 'Core version: $version';
  }

  @override
  String devCoreNotLoaded(String error) {
    return 'Core not loaded: $error';
  }

  @override
  String get notificationsTitle => 'Notifications';

  @override
  String get notificationsEmpty => 'No notifications';

  @override
  String get notificationsCoreNotRunning => 'Core not running.';

  @override
  String get notificationsGeneric => 'Notification';

  @override
  String get notificationsFollow => 'New follower';

  @override
  String get notificationsFollowAccepted => 'Follow accepted';

  @override
  String get notificationsFollowRejected => 'Follow rejected';

  @override
  String get notificationsLike => 'Liked your post';

  @override
  String get notificationsReact => 'Reacted to your post';

  @override
  String get notificationsBoost => 'Boosted your post';

  @override
  String get notificationsMentionOrReply => 'Mention / reply';

  @override
  String get uiSettingsTitle => 'Appearance & language';

  @override
  String get uiSettingsHint => 'Theme, language, density, font size';

  @override
  String get uiLanguage => 'Language';

  @override
  String get uiLanguageSystem => 'System default';

  @override
  String get uiTheme => 'Theme';

  @override
  String get uiThemeSystem => 'System';

  @override
  String get uiThemeLight => 'Light';

  @override
  String get uiThemeDark => 'Dark';

  @override
  String get uiDensity => 'Density';

  @override
  String get uiDensityNormal => 'Normal';

  @override
  String get uiDensityCompact => 'Compact';

  @override
  String get uiAccent => 'Accent color';

  @override
  String get uiFontSize => 'Font size';

  @override
  String get uiFontSizeHint => 'Affects the whole app';

  @override
  String get gifSettingsTitle => 'GIFs';

  @override
  String get gifSettingsHint => 'Giphy API key';

  @override
  String get gifProviderLabel => 'Provider';

  @override
  String get gifProviderHint => 'Use your own API key to enable GIF search.';

  @override
  String get gifProviderTenor => 'Tenor';

  @override
  String get gifProviderGiphy => 'Giphy';

  @override
  String get gifApiKeyLabel => 'API key';

  @override
  String get gifApiKeyHint => 'Paste your Giphy API key';

  @override
  String get gifSettingsDefaultHint =>
      'You can use the default Giphy key for now.';

  @override
  String get gifSettingsUseDefault => 'Use default key';

  @override
  String get composeContentWarningTitle => 'Content warning';

  @override
  String get composeContentWarningHint => 'Hide content behind a warning';

  @override
  String get composeContentWarningTextLabel => 'Warning text';

  @override
  String get composeSensitiveMediaTitle => 'Sensitive media';

  @override
  String get composeSensitiveMediaHint => 'Mark media as sensitive';

  @override
  String get composeEmojiButton => 'Emoji';

  @override
  String get composeMfmCheatsheet => 'MFM cheatsheet';

  @override
  String get composeMfmCheatsheetTitle => 'MFM cheatsheet';

  @override
  String get composeMfmCheatsheetBody =>
      '**bold** -> bold\n*italic* -> italic\n~~strike~~ -> strike\n`code` -> inline code\n#tag -> hashtag\n:emoji: -> custom emoji\nLine breaks: new line';

  @override
  String get close => 'Close';

  @override
  String get composeVisibilityTitle => 'Visibility';

  @override
  String get composeVisibilityHint => 'Who can see this post';

  @override
  String get composeVisibilityPublic => 'Public';

  @override
  String get composeVisibilityHome => 'Home';

  @override
  String get composeVisibilityFollowers => 'Followers only';

  @override
  String get composeVisibilityDirect => 'Direct';

  @override
  String get composeVisibilityDirectLabel => 'Direct recipient';

  @override
  String get composeVisibilityDirectHint => '@user@host or actor URL';

  @override
  String get composeVisibilityDirectMissing => 'Direct recipient is required';

  @override
  String get composeExpand => 'Expand';

  @override
  String get composeExpandTitle => 'Composer';

  @override
  String get uiEmojiPaletteTitle => 'Emoji palette';

  @override
  String get uiEmojiPaletteHint => 'Edit quick emojis';

  @override
  String get emojiPaletteAddLabel => 'Add emoji';

  @override
  String get emojiPaletteAddHint => '😀 or :shortcode:';

  @override
  String get emojiPaletteAddButton => 'Add';

  @override
  String get emojiPaletteEmpty => 'No emojis yet.';

  @override
  String get emojiPickerTitle => 'Emoji';

  @override
  String get emojiPickerClose => 'Close';

  @override
  String get emojiPickerSearchLabel => 'Search';

  @override
  String get emojiPickerSearchHint => 'Emoji or :shortcode:';

  @override
  String get emojiPickerPalette => 'Palette';

  @override
  String get emojiPickerRecent => 'Recent';

  @override
  String get emojiPickerCommon => 'Common';

  @override
  String get emojiPickerCustom => 'Custom emojis';

  @override
  String get reactionPickerTitle => 'Reactions';

  @override
  String get reactionPickerClose => 'Close';

  @override
  String get reactionPickerSearchLabel => 'Search or custom';

  @override
  String get reactionPickerSearchHint => 'Emoji or :shortcode:';

  @override
  String get reactionPickerRecent => 'Recent';

  @override
  String get reactionPickerCommon => 'Common';

  @override
  String get reactionPickerNoteEmojis => 'This note emojis';

  @override
  String get reactionPickerGlobalEmojis => 'Global custom emojis';

  @override
  String get uiEmojiPickerTitle => 'Emoji picker';

  @override
  String get uiEmojiPickerSizeLabel => 'Size';

  @override
  String get uiEmojiPickerColumnsLabel => 'Columns';

  @override
  String get uiEmojiPickerStyleLabel => 'Custom emoji style';

  @override
  String get uiEmojiPickerStyleImage => 'Image';

  @override
  String get uiEmojiPickerStyleText => 'Text';

  @override
  String get uiEmojiPickerPresetLabel => 'Preset';

  @override
  String get uiEmojiPickerPresetCompact => 'Compact';

  @override
  String get uiEmojiPickerPresetComfort => 'Comfort';

  @override
  String get uiEmojiPickerPresetLarge => 'Large';

  @override
  String get uiEmojiPickerPreviewLabel => 'Preview';
}
