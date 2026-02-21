import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:intl/intl.dart' as intl;

import 'app_localizations_en.dart';
import 'app_localizations_it.dart';

// ignore_for_file: type=lint

/// Callers can lookup localized strings with an instance of AppLocalizations
/// returned by `AppLocalizations.of(context)`.
///
/// Applications need to include `AppLocalizations.delegate()` in their app's
/// `localizationDelegates` list, and the locales they support in the app's
/// `supportedLocales` list. For example:
///
/// ```dart
/// import 'gen/app_localizations.dart';
///
/// return MaterialApp(
///   localizationsDelegates: AppLocalizations.localizationsDelegates,
///   supportedLocales: AppLocalizations.supportedLocales,
///   home: MyApplicationHome(),
/// );
/// ```
///
/// ## Update pubspec.yaml
///
/// Please make sure to update your pubspec.yaml to include the following
/// packages:
///
/// ```yaml
/// dependencies:
///   # Internationalization support.
///   flutter_localizations:
///     sdk: flutter
///   intl: any # Use the pinned version from flutter_localizations
///
///   # Rest of dependencies
/// ```
///
/// ## iOS Applications
///
/// iOS applications define key application metadata, including supported
/// locales, in an Info.plist file that is built into the application bundle.
/// To configure the locales supported by your app, you’ll need to edit this
/// file.
///
/// First, open your project’s ios/Runner.xcworkspace Xcode workspace file.
/// Then, in the Project Navigator, open the Info.plist file under the Runner
/// project’s Runner folder.
///
/// Next, select the Information Property List item, select Add Item from the
/// Editor menu, then select Localizations from the pop-up menu.
///
/// Select and expand the newly-created Localizations item then, for each
/// locale your application supports, add a new item and select the locale
/// you wish to add from the pop-up menu in the Value field. This list should
/// be consistent with the languages listed in the AppLocalizations.supportedLocales
/// property.
abstract class AppLocalizations {
  AppLocalizations(String locale)
      : localeName = intl.Intl.canonicalizedLocale(locale.toString());

  final String localeName;

  static AppLocalizations? of(BuildContext context) {
    return Localizations.of<AppLocalizations>(context, AppLocalizations);
  }

  static const LocalizationsDelegate<AppLocalizations> delegate =
      _AppLocalizationsDelegate();

  /// A list of this localizations delegate along with the default localizations
  /// delegates.
  ///
  /// Returns a list of localizations delegates containing this delegate along with
  /// GlobalMaterialLocalizations.delegate, GlobalCupertinoLocalizations.delegate,
  /// and GlobalWidgetsLocalizations.delegate.
  ///
  /// Additional delegates can be added by appending to this list in
  /// MaterialApp. This list does not have to be used at all if a custom list
  /// of delegates is preferred or required.
  static const List<LocalizationsDelegate<dynamic>> localizationsDelegates =
      <LocalizationsDelegate<dynamic>>[
    delegate,
    GlobalMaterialLocalizations.delegate,
    GlobalCupertinoLocalizations.delegate,
    GlobalWidgetsLocalizations.delegate,
  ];

  /// A list of this localizations delegate's supported locales.
  static const List<Locale> supportedLocales = <Locale>[
    Locale('en'),
    Locale('it')
  ];

  /// No description provided for @appTitle.
  ///
  /// In en, this message translates to:
  /// **'Fedi3'**
  String get appTitle;

  /// No description provided for @navTimeline.
  ///
  /// In en, this message translates to:
  /// **'Timeline'**
  String get navTimeline;

  /// No description provided for @navCompose.
  ///
  /// In en, this message translates to:
  /// **'Compose'**
  String get navCompose;

  /// No description provided for @navNotifications.
  ///
  /// In en, this message translates to:
  /// **'Notifications'**
  String get navNotifications;

  /// No description provided for @navSearch.
  ///
  /// In en, this message translates to:
  /// **'Search'**
  String get navSearch;

  /// No description provided for @navChat.
  ///
  /// In en, this message translates to:
  /// **'Chat'**
  String get navChat;

  /// No description provided for @navRelays.
  ///
  /// In en, this message translates to:
  /// **'Relays'**
  String get navRelays;

  /// No description provided for @navSettings.
  ///
  /// In en, this message translates to:
  /// **'Settings'**
  String get navSettings;

  /// No description provided for @save.
  ///
  /// In en, this message translates to:
  /// **'Save'**
  String get save;

  /// No description provided for @cancel.
  ///
  /// In en, this message translates to:
  /// **'Cancel'**
  String get cancel;

  /// No description provided for @ok.
  ///
  /// In en, this message translates to:
  /// **'OK'**
  String get ok;

  /// No description provided for @err.
  ///
  /// In en, this message translates to:
  /// **'Error: {error}'**
  String err(String error);

  /// No description provided for @copy.
  ///
  /// In en, this message translates to:
  /// **'Copy'**
  String get copy;

  /// No description provided for @copied.
  ///
  /// In en, this message translates to:
  /// **'Copied'**
  String get copied;

  /// No description provided for @enabled.
  ///
  /// In en, this message translates to:
  /// **'Enabled'**
  String get enabled;

  /// No description provided for @disabled.
  ///
  /// In en, this message translates to:
  /// **'Disabled'**
  String get disabled;

  /// No description provided for @networkErrorTitle.
  ///
  /// In en, this message translates to:
  /// **'Network issue'**
  String get networkErrorTitle;

  /// No description provided for @networkErrorHint.
  ///
  /// In en, this message translates to:
  /// **'Check your connection and try again.'**
  String get networkErrorHint;

  /// No description provided for @networkErrorRetry.
  ///
  /// In en, this message translates to:
  /// **'Retry'**
  String get networkErrorRetry;

  /// No description provided for @timelineTitle.
  ///
  /// In en, this message translates to:
  /// **'Timeline'**
  String get timelineTitle;

  /// No description provided for @timelineTabHome.
  ///
  /// In en, this message translates to:
  /// **'Home'**
  String get timelineTabHome;

  /// No description provided for @timelineTabLocal.
  ///
  /// In en, this message translates to:
  /// **'Local'**
  String get timelineTabLocal;

  /// No description provided for @timelineTabSocial.
  ///
  /// In en, this message translates to:
  /// **'Social'**
  String get timelineTabSocial;

  /// No description provided for @timelineTabFederated.
  ///
  /// In en, this message translates to:
  /// **'Federated'**
  String get timelineTabFederated;

  /// No description provided for @searchTitle.
  ///
  /// In en, this message translates to:
  /// **'Search'**
  String get searchTitle;

  /// No description provided for @searchHint.
  ///
  /// In en, this message translates to:
  /// **'Search posts, users, hashtags'**
  String get searchHint;

  /// No description provided for @searchTabPosts.
  ///
  /// In en, this message translates to:
  /// **'Posts'**
  String get searchTabPosts;

  /// No description provided for @searchTabUsers.
  ///
  /// In en, this message translates to:
  /// **'Users'**
  String get searchTabUsers;

  /// No description provided for @searchTabHashtags.
  ///
  /// In en, this message translates to:
  /// **'Hashtags'**
  String get searchTabHashtags;

  /// No description provided for @searchEmpty.
  ///
  /// In en, this message translates to:
  /// **'Type to search'**
  String get searchEmpty;

  /// No description provided for @searchNoResults.
  ///
  /// In en, this message translates to:
  /// **'No results'**
  String get searchNoResults;

  /// No description provided for @searchShowingFor.
  ///
  /// In en, this message translates to:
  /// **'Showing results for'**
  String get searchShowingFor;

  /// No description provided for @searchSourceAll.
  ///
  /// In en, this message translates to:
  /// **'All'**
  String get searchSourceAll;

  /// No description provided for @searchSourceLocal.
  ///
  /// In en, this message translates to:
  /// **'Local'**
  String get searchSourceLocal;

  /// No description provided for @searchSourceRelay.
  ///
  /// In en, this message translates to:
  /// **'Relay'**
  String get searchSourceRelay;

  /// No description provided for @searchTagCount.
  ///
  /// In en, this message translates to:
  /// **'{count} posts'**
  String searchTagCount(int count);

  /// No description provided for @chatTitle.
  ///
  /// In en, this message translates to:
  /// **'Chat'**
  String get chatTitle;

  /// No description provided for @chatThreadTitle.
  ///
  /// In en, this message translates to:
  /// **'Conversation'**
  String get chatThreadTitle;

  /// No description provided for @chatNewTitle.
  ///
  /// In en, this message translates to:
  /// **'New chat'**
  String get chatNewTitle;

  /// No description provided for @chatNewTooltip.
  ///
  /// In en, this message translates to:
  /// **'Start a new chat'**
  String get chatNewTooltip;

  /// No description provided for @chatNewMissingFields.
  ///
  /// In en, this message translates to:
  /// **'Add recipients and a message.'**
  String get chatNewMissingFields;

  /// No description provided for @chatNewFailed.
  ///
  /// In en, this message translates to:
  /// **'Failed to create chat.'**
  String get chatNewFailed;

  /// No description provided for @chatRecipients.
  ///
  /// In en, this message translates to:
  /// **'Recipients'**
  String get chatRecipients;

  /// No description provided for @chatRecipientsHint.
  ///
  /// In en, this message translates to:
  /// **'e.g. @alice@example.com, https://server/users/bob'**
  String get chatRecipientsHint;

  /// No description provided for @chatMessage.
  ///
  /// In en, this message translates to:
  /// **'Message'**
  String get chatMessage;

  /// No description provided for @chatMessageHint.
  ///
  /// In en, this message translates to:
  /// **'Write a message…'**
  String get chatMessageHint;

  /// No description provided for @chatCreate.
  ///
  /// In en, this message translates to:
  /// **'Create'**
  String get chatCreate;

  /// No description provided for @chatSend.
  ///
  /// In en, this message translates to:
  /// **'Send'**
  String get chatSend;

  /// No description provided for @chatRefresh.
  ///
  /// In en, this message translates to:
  /// **'Refresh'**
  String get chatRefresh;

  /// No description provided for @chatThreadsEmpty.
  ///
  /// In en, this message translates to:
  /// **'No chats yet'**
  String get chatThreadsEmpty;

  /// No description provided for @chatEmpty.
  ///
  /// In en, this message translates to:
  /// **'No messages yet'**
  String get chatEmpty;

  /// No description provided for @chatNoMessages.
  ///
  /// In en, this message translates to:
  /// **'No messages'**
  String get chatNoMessages;

  /// No description provided for @chatDirectMessage.
  ///
  /// In en, this message translates to:
  /// **'Direct message'**
  String get chatDirectMessage;

  /// No description provided for @chatGroup.
  ///
  /// In en, this message translates to:
  /// **'Group chat'**
  String get chatGroup;

  /// No description provided for @chatMessageDeleted.
  ///
  /// In en, this message translates to:
  /// **'Message deleted'**
  String get chatMessageDeleted;

  /// No description provided for @chatMessageEmpty.
  ///
  /// In en, this message translates to:
  /// **'Empty message'**
  String get chatMessageEmpty;

  /// No description provided for @chatEdit.
  ///
  /// In en, this message translates to:
  /// **'Edit'**
  String get chatEdit;

  /// No description provided for @chatDelete.
  ///
  /// In en, this message translates to:
  /// **'Delete'**
  String get chatDelete;

  /// No description provided for @chatEditTitle.
  ///
  /// In en, this message translates to:
  /// **'Edit message'**
  String get chatEditTitle;

  /// No description provided for @chatDeleteTitle.
  ///
  /// In en, this message translates to:
  /// **'Delete message'**
  String get chatDeleteTitle;

  /// No description provided for @chatDeleteHint.
  ///
  /// In en, this message translates to:
  /// **'This will remove the message for everyone.'**
  String get chatDeleteHint;

  /// No description provided for @chatSave.
  ///
  /// In en, this message translates to:
  /// **'Save'**
  String get chatSave;

  /// No description provided for @chatNewMessage.
  ///
  /// In en, this message translates to:
  /// **'New message'**
  String get chatNewMessage;

  /// No description provided for @chatNewMessageBody.
  ///
  /// In en, this message translates to:
  /// **'You have a new message.'**
  String get chatNewMessageBody;

  /// No description provided for @chatOpen.
  ///
  /// In en, this message translates to:
  /// **'Open'**
  String get chatOpen;

  /// No description provided for @chatGif.
  ///
  /// In en, this message translates to:
  /// **'GIF'**
  String get chatGif;

  /// No description provided for @chatGifSearchHint.
  ///
  /// In en, this message translates to:
  /// **'Search GIFs…'**
  String get chatGifSearchHint;

  /// No description provided for @chatGifEmpty.
  ///
  /// In en, this message translates to:
  /// **'No GIFs found'**
  String get chatGifEmpty;

  /// No description provided for @chatGifMissingKey.
  ///
  /// In en, this message translates to:
  /// **'Add a GIF API key in Settings to enable search.'**
  String get chatGifMissingKey;

  /// No description provided for @chatStatusPending.
  ///
  /// In en, this message translates to:
  /// **'Pending'**
  String get chatStatusPending;

  /// No description provided for @chatStatusQueued.
  ///
  /// In en, this message translates to:
  /// **'Queued'**
  String get chatStatusQueued;

  /// No description provided for @chatStatusSent.
  ///
  /// In en, this message translates to:
  /// **'Sent'**
  String get chatStatusSent;

  /// No description provided for @chatStatusDelivered.
  ///
  /// In en, this message translates to:
  /// **'Delivered'**
  String get chatStatusDelivered;

  /// No description provided for @chatStatusSeen.
  ///
  /// In en, this message translates to:
  /// **'Seen'**
  String get chatStatusSeen;

  /// No description provided for @chatReply.
  ///
  /// In en, this message translates to:
  /// **'Reply'**
  String get chatReply;

  /// No description provided for @chatReplyClear.
  ///
  /// In en, this message translates to:
  /// **'Cancel reply'**
  String get chatReplyClear;

  /// No description provided for @chatReplyAttachment.
  ///
  /// In en, this message translates to:
  /// **'Attachment'**
  String get chatReplyAttachment;

  /// No description provided for @chatReplyUnknown.
  ///
  /// In en, this message translates to:
  /// **'Original message'**
  String get chatReplyUnknown;

  /// No description provided for @chatSenderMe.
  ///
  /// In en, this message translates to:
  /// **'me'**
  String get chatSenderMe;

  /// No description provided for @chatMembers.
  ///
  /// In en, this message translates to:
  /// **'Members'**
  String get chatMembers;

  /// No description provided for @chatRename.
  ///
  /// In en, this message translates to:
  /// **'Rename chat'**
  String get chatRename;

  /// No description provided for @chatRenameHint.
  ///
  /// In en, this message translates to:
  /// **'New chat title'**
  String get chatRenameHint;

  /// No description provided for @chatAddMember.
  ///
  /// In en, this message translates to:
  /// **'Add member'**
  String get chatAddMember;

  /// No description provided for @chatAddMemberHint.
  ///
  /// In en, this message translates to:
  /// **'Search or paste an actor'**
  String get chatAddMemberHint;

  /// No description provided for @chatRemoveMember.
  ///
  /// In en, this message translates to:
  /// **'Remove member'**
  String get chatRemoveMember;

  /// No description provided for @chatReact.
  ///
  /// In en, this message translates to:
  /// **'React'**
  String get chatReact;

  /// No description provided for @chatDeleteThread.
  ///
  /// In en, this message translates to:
  /// **'Delete chat'**
  String get chatDeleteThread;

  /// No description provided for @chatDeleteThreadHint.
  ///
  /// In en, this message translates to:
  /// **'This will delete the entire chat thread for everyone.'**
  String get chatDeleteThreadHint;

  /// No description provided for @chatLeaveThreadOption.
  ///
  /// In en, this message translates to:
  /// **'Leave chat'**
  String get chatLeaveThreadOption;

  /// No description provided for @chatArchiveThreadOption.
  ///
  /// In en, this message translates to:
  /// **'Archive chat'**
  String get chatArchiveThreadOption;

  /// No description provided for @chatLeaveThreadSuccess.
  ///
  /// In en, this message translates to:
  /// **'You left the chat.'**
  String get chatLeaveThreadSuccess;

  /// No description provided for @chatLeaveThreadFailed.
  ///
  /// In en, this message translates to:
  /// **'Could not leave the chat.'**
  String get chatLeaveThreadFailed;

  /// No description provided for @chatArchiveThreadSuccess.
  ///
  /// In en, this message translates to:
  /// **'Chat archived.'**
  String get chatArchiveThreadSuccess;

  /// No description provided for @chatArchiveThreadFailed.
  ///
  /// In en, this message translates to:
  /// **'Could not archive the chat.'**
  String get chatArchiveThreadFailed;

  /// No description provided for @chatUnarchiveThreadSuccess.
  ///
  /// In en, this message translates to:
  /// **'Chat restored.'**
  String get chatUnarchiveThreadSuccess;

  /// No description provided for @chatUnarchiveThreadFailed.
  ///
  /// In en, this message translates to:
  /// **'Could not restore the chat.'**
  String get chatUnarchiveThreadFailed;

  /// No description provided for @chatThreadsActive.
  ///
  /// In en, this message translates to:
  /// **'Active'**
  String get chatThreadsActive;

  /// No description provided for @chatThreadsArchived.
  ///
  /// In en, this message translates to:
  /// **'Archived'**
  String get chatThreadsArchived;

  /// No description provided for @chatUnarchiveThreadOption.
  ///
  /// In en, this message translates to:
  /// **'Unarchive chat'**
  String get chatUnarchiveThreadOption;

  /// No description provided for @chatPin.
  ///
  /// In en, this message translates to:
  /// **'Pin chat'**
  String get chatPin;

  /// No description provided for @chatUnpin.
  ///
  /// In en, this message translates to:
  /// **'Unpin chat'**
  String get chatUnpin;

  /// No description provided for @chatTyping.
  ///
  /// In en, this message translates to:
  /// **'{name} is typing…'**
  String chatTyping(String name);

  /// No description provided for @chatTypingMany.
  ///
  /// In en, this message translates to:
  /// **'{names} are typing…'**
  String chatTypingMany(String names);

  /// No description provided for @timelineHomeTooltip.
  ///
  /// In en, this message translates to:
  /// **'Generic timeline (Relay + Legacy). Shows incoming/outgoing interactions, including with users you don\'t follow and who don\'t follow you.'**
  String get timelineHomeTooltip;

  /// No description provided for @timelineLocalTooltip.
  ///
  /// In en, this message translates to:
  /// **'Follower/Following timeline (Relay + Legacy). Only profiles with a follow/follower relationship.'**
  String get timelineLocalTooltip;

  /// No description provided for @timelineSocialTooltip.
  ///
  /// In en, this message translates to:
  /// **'Social timeline (Relay). Best-effort global feed via relay federation.'**
  String get timelineSocialTooltip;

  /// No description provided for @timelineFederatedTooltip.
  ///
  /// In en, this message translates to:
  /// **'Federated timeline (Relay + Legacy). Public content seen by your instance across federation.'**
  String get timelineFederatedTooltip;

  /// No description provided for @timelineLayoutColumns.
  ///
  /// In en, this message translates to:
  /// **'Switch to columns'**
  String get timelineLayoutColumns;

  /// No description provided for @timelineLayoutTabs.
  ///
  /// In en, this message translates to:
  /// **'Switch to tabs'**
  String get timelineLayoutTabs;

  /// No description provided for @timelineFilters.
  ///
  /// In en, this message translates to:
  /// **'Filters'**
  String get timelineFilters;

  /// No description provided for @timelineFilterMedia.
  ///
  /// In en, this message translates to:
  /// **'Media'**
  String get timelineFilterMedia;

  /// No description provided for @timelineFilterReply.
  ///
  /// In en, this message translates to:
  /// **'Reply'**
  String get timelineFilterReply;

  /// No description provided for @timelineFilterBoost.
  ///
  /// In en, this message translates to:
  /// **'Boost'**
  String get timelineFilterBoost;

  /// No description provided for @timelineFilterMention.
  ///
  /// In en, this message translates to:
  /// **'Mention'**
  String get timelineFilterMention;

  /// No description provided for @coreStart.
  ///
  /// In en, this message translates to:
  /// **'Start core'**
  String get coreStart;

  /// No description provided for @coreStop.
  ///
  /// In en, this message translates to:
  /// **'Stop core'**
  String get coreStop;

  /// No description provided for @listNoItems.
  ///
  /// In en, this message translates to:
  /// **'No items'**
  String get listNoItems;

  /// No description provided for @listEnd.
  ///
  /// In en, this message translates to:
  /// **'End'**
  String get listEnd;

  /// No description provided for @listLoadMore.
  ///
  /// In en, this message translates to:
  /// **'Load more'**
  String get listLoadMore;

  /// No description provided for @timelineNewPosts.
  ///
  /// In en, this message translates to:
  /// **'{count} new posts'**
  String timelineNewPosts(int count);

  /// No description provided for @timelineShowNewPosts.
  ///
  /// In en, this message translates to:
  /// **'Show'**
  String get timelineShowNewPosts;

  /// No description provided for @timelineShowNewPostsHint.
  ///
  /// In en, this message translates to:
  /// **'Click to show without reloading'**
  String get timelineShowNewPostsHint;

  /// No description provided for @timelineRefreshTitle.
  ///
  /// In en, this message translates to:
  /// **'Refresh'**
  String get timelineRefreshTitle;

  /// No description provided for @timelineRefreshHint.
  ///
  /// In en, this message translates to:
  /// **'Force reload and pull from legacy while you were offline'**
  String get timelineRefreshHint;

  /// No description provided for @activityBoost.
  ///
  /// In en, this message translates to:
  /// **'Boost'**
  String get activityBoost;

  /// No description provided for @activityReply.
  ///
  /// In en, this message translates to:
  /// **'Reply'**
  String get activityReply;

  /// No description provided for @activityLike.
  ///
  /// In en, this message translates to:
  /// **'Like'**
  String get activityLike;

  /// No description provided for @activityReact.
  ///
  /// In en, this message translates to:
  /// **'React'**
  String get activityReact;

  /// No description provided for @activityReplyTitle.
  ///
  /// In en, this message translates to:
  /// **'Reply'**
  String get activityReplyTitle;

  /// No description provided for @activityReplyHint.
  ///
  /// In en, this message translates to:
  /// **'Write a reply.'**
  String get activityReplyHint;

  /// No description provided for @activityCancel.
  ///
  /// In en, this message translates to:
  /// **'Cancel'**
  String get activityCancel;

  /// No description provided for @activitySend.
  ///
  /// In en, this message translates to:
  /// **'Send'**
  String get activitySend;

  /// No description provided for @activityUnsupported.
  ///
  /// In en, this message translates to:
  /// **'Unsupported item'**
  String get activityUnsupported;

  /// No description provided for @activityViewRaw.
  ///
  /// In en, this message translates to:
  /// **'View raw'**
  String get activityViewRaw;

  /// No description provided for @activityHideRaw.
  ///
  /// In en, this message translates to:
  /// **'Hide raw'**
  String get activityHideRaw;

  /// No description provided for @noteInReplyTo.
  ///
  /// In en, this message translates to:
  /// **'In reply to'**
  String get noteInReplyTo;

  /// No description provided for @noteBoostedBy.
  ///
  /// In en, this message translates to:
  /// **'Boosted by {name}'**
  String noteBoostedBy(String name);

  /// No description provided for @noteThreadTitle.
  ///
  /// In en, this message translates to:
  /// **'Thread'**
  String get noteThreadTitle;

  /// No description provided for @noteThreadUp.
  ///
  /// In en, this message translates to:
  /// **'In reply to'**
  String get noteThreadUp;

  /// No description provided for @noteShowThread.
  ///
  /// In en, this message translates to:
  /// **'Show thread'**
  String get noteShowThread;

  /// No description provided for @noteHideThread.
  ///
  /// In en, this message translates to:
  /// **'Hide thread'**
  String get noteHideThread;

  /// No description provided for @noteLoadingThread.
  ///
  /// In en, this message translates to:
  /// **'Loading…'**
  String get noteLoadingThread;

  /// No description provided for @noteShowReplies.
  ///
  /// In en, this message translates to:
  /// **'Show replies'**
  String get noteShowReplies;

  /// No description provided for @noteHideReplies.
  ///
  /// In en, this message translates to:
  /// **'Hide replies'**
  String get noteHideReplies;

  /// No description provided for @noteLoadingReplies.
  ///
  /// In en, this message translates to:
  /// **'Loading replies…'**
  String get noteLoadingReplies;

  /// No description provided for @noteReactionLoading.
  ///
  /// In en, this message translates to:
  /// **'Loading reactions…'**
  String get noteReactionLoading;

  /// No description provided for @noteReactionAdd.
  ///
  /// In en, this message translates to:
  /// **'Add reaction'**
  String get noteReactionAdd;

  /// No description provided for @noteContentWarning.
  ///
  /// In en, this message translates to:
  /// **'Content warning'**
  String get noteContentWarning;

  /// No description provided for @noteShowContent.
  ///
  /// In en, this message translates to:
  /// **'Show'**
  String get noteShowContent;

  /// No description provided for @noteHideContent.
  ///
  /// In en, this message translates to:
  /// **'Hide'**
  String get noteHideContent;

  /// No description provided for @noteActions.
  ///
  /// In en, this message translates to:
  /// **'Actions'**
  String get noteActions;

  /// No description provided for @noteEdit.
  ///
  /// In en, this message translates to:
  /// **'Edit'**
  String get noteEdit;

  /// No description provided for @noteDelete.
  ///
  /// In en, this message translates to:
  /// **'Delete'**
  String get noteDelete;

  /// No description provided for @noteDeleted.
  ///
  /// In en, this message translates to:
  /// **'Note deleted'**
  String get noteDeleted;

  /// No description provided for @noteEditTitle.
  ///
  /// In en, this message translates to:
  /// **'Edit note'**
  String get noteEditTitle;

  /// No description provided for @noteEditContentHint.
  ///
  /// In en, this message translates to:
  /// **'Edit content'**
  String get noteEditContentHint;

  /// No description provided for @noteEditSummaryHint.
  ///
  /// In en, this message translates to:
  /// **'Content warning (optional)'**
  String get noteEditSummaryHint;

  /// No description provided for @noteSensitiveLabel.
  ///
  /// In en, this message translates to:
  /// **'Sensitive content'**
  String get noteSensitiveLabel;

  /// No description provided for @noteEditSave.
  ///
  /// In en, this message translates to:
  /// **'Save'**
  String get noteEditSave;

  /// No description provided for @noteEditMissingAudience.
  ///
  /// In en, this message translates to:
  /// **'Missing audience'**
  String get noteEditMissingAudience;

  /// No description provided for @noteDeleteTitle.
  ///
  /// In en, this message translates to:
  /// **'Delete note'**
  String get noteDeleteTitle;

  /// No description provided for @noteDeleteHint.
  ///
  /// In en, this message translates to:
  /// **'Are you sure you want to delete this note?'**
  String get noteDeleteHint;

  /// No description provided for @noteDeleteConfirm.
  ///
  /// In en, this message translates to:
  /// **'Delete'**
  String get noteDeleteConfirm;

  /// No description provided for @noteDeleteMissingAudience.
  ///
  /// In en, this message translates to:
  /// **'Missing audience'**
  String get noteDeleteMissingAudience;

  /// No description provided for @timeAgoJustNow.
  ///
  /// In en, this message translates to:
  /// **'just now'**
  String get timeAgoJustNow;

  /// No description provided for @timeAgoMinutes.
  ///
  /// In en, this message translates to:
  /// **'{count,plural,=1{1 minute ago}other{{count} minutes ago}}'**
  String timeAgoMinutes(int count);

  /// No description provided for @timeAgoHours.
  ///
  /// In en, this message translates to:
  /// **'{count,plural,=1{1 hour ago}other{{count} hours ago}}'**
  String timeAgoHours(int count);

  /// No description provided for @timeAgoDays.
  ///
  /// In en, this message translates to:
  /// **'{count,plural,=1{1 day ago}other{{count} days ago}}'**
  String timeAgoDays(int count);

  /// No description provided for @timeAgoMonths.
  ///
  /// In en, this message translates to:
  /// **'{count,plural,=1{1 month ago}other{{count} months ago}}'**
  String timeAgoMonths(int count);

  /// No description provided for @timeAgoYears.
  ///
  /// In en, this message translates to:
  /// **'{count,plural,=1{1 year ago}other{{count} years ago}}'**
  String timeAgoYears(int count);

  /// No description provided for @firstRunTitle.
  ///
  /// In en, this message translates to:
  /// **'Welcome'**
  String get firstRunTitle;

  /// No description provided for @firstRunIntro.
  ///
  /// In en, this message translates to:
  /// **'Do you want to use an existing account or create a new one?'**
  String get firstRunIntro;

  /// No description provided for @firstRunLoginTitle.
  ///
  /// In en, this message translates to:
  /// **'Log in (existing setup)'**
  String get firstRunLoginTitle;

  /// No description provided for @firstRunLoginHint.
  ///
  /// In en, this message translates to:
  /// **'Enter the settings of an existing account/setup.'**
  String get firstRunLoginHint;

  /// No description provided for @firstRunCreateTitle.
  ///
  /// In en, this message translates to:
  /// **'Create new account'**
  String get firstRunCreateTitle;

  /// No description provided for @firstRunCreateHint.
  ///
  /// In en, this message translates to:
  /// **'Create a new local identity and configure relay endpoints.'**
  String get firstRunCreateHint;

  /// No description provided for @firstRunRelayPreviewTitle.
  ///
  /// In en, this message translates to:
  /// **'Relay network preview'**
  String get firstRunRelayPreviewTitle;

  /// No description provided for @firstRunRelayPreviewRelays.
  ///
  /// In en, this message translates to:
  /// **'Relays'**
  String get firstRunRelayPreviewRelays;

  /// No description provided for @firstRunRelayPreviewPeers.
  ///
  /// In en, this message translates to:
  /// **'Peers'**
  String get firstRunRelayPreviewPeers;

  /// No description provided for @firstRunRelayPreviewEmpty.
  ///
  /// In en, this message translates to:
  /// **'No data yet'**
  String get firstRunRelayPreviewEmpty;

  /// No description provided for @firstRunRelayPreviewError.
  ///
  /// In en, this message translates to:
  /// **'Failed to load relay preview.'**
  String get firstRunRelayPreviewError;

  /// No description provided for @onboardingCreateTitle.
  ///
  /// In en, this message translates to:
  /// **'Create account'**
  String get onboardingCreateTitle;

  /// No description provided for @onboardingCreateIntro.
  ///
  /// In en, this message translates to:
  /// **'Fill the required fields to create a new setup. The core will start automatically after saving.'**
  String get onboardingCreateIntro;

  /// No description provided for @onboardingLoginTitle.
  ///
  /// In en, this message translates to:
  /// **'Log in'**
  String get onboardingLoginTitle;

  /// No description provided for @onboardingLoginIntro.
  ///
  /// In en, this message translates to:
  /// **'Enter your existing setup fields. The core will start automatically after saving.'**
  String get onboardingLoginIntro;

  /// No description provided for @onboardingImportBackup.
  ///
  /// In en, this message translates to:
  /// **'Import backup JSON'**
  String get onboardingImportBackup;

  /// No description provided for @onboardingImportBackupCloud.
  ///
  /// In en, this message translates to:
  /// **'Restore from relay backup'**
  String get onboardingImportBackupCloud;

  /// No description provided for @onboardingRelaySelect.
  ///
  /// In en, this message translates to:
  /// **'Relay selection'**
  String get onboardingRelaySelect;

  /// No description provided for @onboardingRelayDiscover.
  ///
  /// In en, this message translates to:
  /// **'Discover'**
  String get onboardingRelayDiscover;

  /// No description provided for @onboardingRelayDiscovering.
  ///
  /// In en, this message translates to:
  /// **'Discovering…'**
  String get onboardingRelayDiscovering;

  /// No description provided for @onboardingRelayListHint.
  ///
  /// In en, this message translates to:
  /// **'Pick a relay from the shared list or enter one manually.'**
  String get onboardingRelayListHint;

  /// No description provided for @onboardingRelayPick.
  ///
  /// In en, this message translates to:
  /// **'Select a relay'**
  String get onboardingRelayPick;

  /// No description provided for @onboardingRelayCustom.
  ///
  /// In en, this message translates to:
  /// **'Use custom relay settings'**
  String get onboardingRelayCustom;

  /// No description provided for @composeTitle.
  ///
  /// In en, this message translates to:
  /// **'Compose'**
  String get composeTitle;

  /// No description provided for @composeCharCount.
  ///
  /// In en, this message translates to:
  /// **'{used}/{max}'**
  String composeCharCount(int used, int max);

  /// No description provided for @composeCoreNotRunning.
  ///
  /// In en, this message translates to:
  /// **'Core not running: go to {settings} and press {start}.'**
  String composeCoreNotRunning(String settings, String start);

  /// No description provided for @composeWhatsHappening.
  ///
  /// In en, this message translates to:
  /// **'Write what you think.'**
  String get composeWhatsHappening;

  /// No description provided for @composePublic.
  ///
  /// In en, this message translates to:
  /// **'Public'**
  String get composePublic;

  /// No description provided for @composePublicHint.
  ///
  /// In en, this message translates to:
  /// **'If off: followers-only'**
  String get composePublicHint;

  /// No description provided for @composeAddMediaPath.
  ///
  /// In en, this message translates to:
  /// **'Add media (path)'**
  String get composeAddMediaPath;

  /// No description provided for @composeAddMedia.
  ///
  /// In en, this message translates to:
  /// **'Attach media'**
  String get composeAddMedia;

  /// No description provided for @composeDropHere.
  ///
  /// In en, this message translates to:
  /// **'Drop files to attach'**
  String get composeDropHere;

  /// No description provided for @composeClearDraft.
  ///
  /// In en, this message translates to:
  /// **'Clear draft'**
  String get composeClearDraft;

  /// No description provided for @composeDraftSaved.
  ///
  /// In en, this message translates to:
  /// **'Draft saved'**
  String get composeDraftSaved;

  /// No description provided for @composeDraftRestored.
  ///
  /// In en, this message translates to:
  /// **'Draft restored'**
  String get composeDraftRestored;

  /// No description provided for @composeDraftCleared.
  ///
  /// In en, this message translates to:
  /// **'Draft cleared'**
  String get composeDraftCleared;

  /// No description provided for @composeDraftResumeTooltip.
  ///
  /// In en, this message translates to:
  /// **'Resume draft'**
  String get composeDraftResumeTooltip;

  /// No description provided for @composeDraftDeleteTooltip.
  ///
  /// In en, this message translates to:
  /// **'Delete draft'**
  String get composeDraftDeleteTooltip;

  /// No description provided for @composeQuickHint.
  ///
  /// In en, this message translates to:
  /// **'Open the quick composer'**
  String get composeQuickHint;

  /// No description provided for @translationTitle.
  ///
  /// In en, this message translates to:
  /// **'Translation'**
  String get translationTitle;

  /// No description provided for @translationHint.
  ///
  /// In en, this message translates to:
  /// **'DeepL or DeepLX settings'**
  String get translationHint;

  /// No description provided for @translationProviderLabel.
  ///
  /// In en, this message translates to:
  /// **'Provider'**
  String get translationProviderLabel;

  /// No description provided for @translationProviderDeepL.
  ///
  /// In en, this message translates to:
  /// **'DeepL'**
  String get translationProviderDeepL;

  /// No description provided for @translationProviderDeepLX.
  ///
  /// In en, this message translates to:
  /// **'DeepLX'**
  String get translationProviderDeepLX;

  /// No description provided for @translationAuthKeyLabel.
  ///
  /// In en, this message translates to:
  /// **'Auth key'**
  String get translationAuthKeyLabel;

  /// No description provided for @translationAuthKeyHint.
  ///
  /// In en, this message translates to:
  /// **'DeepL API key'**
  String get translationAuthKeyHint;

  /// No description provided for @translationUseProLabel.
  ///
  /// In en, this message translates to:
  /// **'Pro account'**
  String get translationUseProLabel;

  /// No description provided for @translationUseProHint.
  ///
  /// In en, this message translates to:
  /// **'Use api.deepl.com instead of api-free.deepl.com'**
  String get translationUseProHint;

  /// No description provided for @translationDeepLXUrlLabel.
  ///
  /// In en, this message translates to:
  /// **'DeepLX endpoint'**
  String get translationDeepLXUrlLabel;

  /// No description provided for @translationDeepLXUrlHint.
  ///
  /// In en, this message translates to:
  /// **'https://api.deeplx.org/translate'**
  String get translationDeepLXUrlHint;

  /// No description provided for @translationTimeoutLabel.
  ///
  /// In en, this message translates to:
  /// **'Timeout'**
  String get translationTimeoutLabel;

  /// No description provided for @translationTimeoutValue.
  ///
  /// In en, this message translates to:
  /// **'{seconds, plural, one {# second} other {# seconds}}'**
  String translationTimeoutValue(int seconds);

  /// No description provided for @translationTargetLabel.
  ///
  /// In en, this message translates to:
  /// **'Target language: {lang}'**
  String translationTargetLabel(String lang);

  /// No description provided for @noteTranslate.
  ///
  /// In en, this message translates to:
  /// **'Translate'**
  String get noteTranslate;

  /// No description provided for @noteShowTranslation.
  ///
  /// In en, this message translates to:
  /// **'Show translation'**
  String get noteShowTranslation;

  /// No description provided for @noteShowOriginal.
  ///
  /// In en, this message translates to:
  /// **'Show original'**
  String get noteShowOriginal;

  /// No description provided for @noteTranslatedFrom.
  ///
  /// In en, this message translates to:
  /// **'Translated from {lang}'**
  String noteTranslatedFrom(String lang);

  /// No description provided for @noteTranslateFailed.
  ///
  /// In en, this message translates to:
  /// **'Translation failed: {error}'**
  String noteTranslateFailed(String error);

  /// No description provided for @composePost.
  ///
  /// In en, this message translates to:
  /// **'Post'**
  String get composePost;

  /// No description provided for @composeAttachments.
  ///
  /// In en, this message translates to:
  /// **'Attachments'**
  String get composeAttachments;

  /// No description provided for @composeMediaId.
  ///
  /// In en, this message translates to:
  /// **'mediaId={id}'**
  String composeMediaId(String id);

  /// No description provided for @composeFileFallback.
  ///
  /// In en, this message translates to:
  /// **'file'**
  String get composeFileFallback;

  /// No description provided for @composeMediaFilePathLabel.
  ///
  /// In en, this message translates to:
  /// **'Media file path (desktop/dev)'**
  String get composeMediaFilePathLabel;

  /// No description provided for @composeMediaFilePathHint.
  ///
  /// In en, this message translates to:
  /// **'C:\\\\path\\\\img.png or /home/user/img.png'**
  String get composeMediaFilePathHint;

  /// No description provided for @composeNotUploaded.
  ///
  /// In en, this message translates to:
  /// **'Not uploaded'**
  String get composeNotUploaded;

  /// No description provided for @composeQueuedOk.
  ///
  /// In en, this message translates to:
  /// **'OK: queued for delivery'**
  String get composeQueuedOk;

  /// No description provided for @composeErrEmptyContent.
  ///
  /// In en, this message translates to:
  /// **'ERR: empty content'**
  String get composeErrEmptyContent;

  /// No description provided for @composeErrUnableReadFile.
  ///
  /// In en, this message translates to:
  /// **'ERR: unable to read file: {error}'**
  String composeErrUnableReadFile(String error);

  /// No description provided for @composeErrUnablePickFile.
  ///
  /// In en, this message translates to:
  /// **'ERR: file picker failed: {error}'**
  String composeErrUnablePickFile(String error);

  /// No description provided for @composeErrInvalidMediaType.
  ///
  /// In en, this message translates to:
  /// **'ERR: invalid media type: {type}'**
  String composeErrInvalidMediaType(String type);

  /// No description provided for @composeErrGeneric.
  ///
  /// In en, this message translates to:
  /// **'ERR: {error}'**
  String composeErrGeneric(String error);

  /// No description provided for @settingsTitle.
  ///
  /// In en, this message translates to:
  /// **'Settings'**
  String get settingsTitle;

  /// No description provided for @settingsCore.
  ///
  /// In en, this message translates to:
  /// **'Core'**
  String get settingsCore;

  /// No description provided for @settingsCoreRunning.
  ///
  /// In en, this message translates to:
  /// **'Running (handle={handle})'**
  String settingsCoreRunning(int handle);

  /// No description provided for @settingsCoreStopped.
  ///
  /// In en, this message translates to:
  /// **'Stopped'**
  String get settingsCoreStopped;

  /// No description provided for @settingsAccount.
  ///
  /// In en, this message translates to:
  /// **'Account'**
  String get settingsAccount;

  /// No description provided for @settingsFollowSection.
  ///
  /// In en, this message translates to:
  /// **'Follow / Unfollow'**
  String get settingsFollowSection;

  /// No description provided for @settingsActorUrlLabel.
  ///
  /// In en, this message translates to:
  /// **'Actor URL (https://...)'**
  String get settingsActorUrlLabel;

  /// No description provided for @settingsFollow.
  ///
  /// In en, this message translates to:
  /// **'Follow'**
  String get settingsFollow;

  /// No description provided for @settingsUnfollow.
  ///
  /// In en, this message translates to:
  /// **'Unfollow'**
  String get settingsUnfollow;

  /// No description provided for @settingsAdvancedDev.
  ///
  /// In en, this message translates to:
  /// **'Advanced (dev)'**
  String get settingsAdvancedDev;

  /// No description provided for @settingsAdvancedDevHint.
  ///
  /// In en, this message translates to:
  /// **'Start/Stop + migration status'**
  String get settingsAdvancedDevHint;

  /// No description provided for @settingsResetApp.
  ///
  /// In en, this message translates to:
  /// **'Reset app (clear config)'**
  String get settingsResetApp;

  /// No description provided for @profileEditTitle.
  ///
  /// In en, this message translates to:
  /// **'Profile'**
  String get profileEditTitle;

  /// No description provided for @profileEditHint.
  ///
  /// In en, this message translates to:
  /// **'Public profile, avatar, banner, fields'**
  String get profileEditHint;

  /// No description provided for @profileDisplayName.
  ///
  /// In en, this message translates to:
  /// **'Display name'**
  String get profileDisplayName;

  /// No description provided for @profileBio.
  ///
  /// In en, this message translates to:
  /// **'Bio'**
  String get profileBio;

  /// No description provided for @profileFollowers.
  ///
  /// In en, this message translates to:
  /// **'Followers'**
  String get profileFollowers;

  /// No description provided for @profileFollowing.
  ///
  /// In en, this message translates to:
  /// **'Following'**
  String get profileFollowing;

  /// No description provided for @profileFeatured.
  ///
  /// In en, this message translates to:
  /// **'Featured'**
  String get profileFeatured;

  /// No description provided for @profileAliases.
  ///
  /// In en, this message translates to:
  /// **'Also known as'**
  String get profileAliases;

  /// No description provided for @profileFollowPending.
  ///
  /// In en, this message translates to:
  /// **'Pending'**
  String get profileFollowPending;

  /// No description provided for @profileMovedTo.
  ///
  /// In en, this message translates to:
  /// **'Moved to {actor}'**
  String profileMovedTo(String actor);

  /// No description provided for @profileAvatar.
  ///
  /// In en, this message translates to:
  /// **'Avatar'**
  String get profileAvatar;

  /// No description provided for @profileBanner.
  ///
  /// In en, this message translates to:
  /// **'Banner'**
  String get profileBanner;

  /// No description provided for @profilePickFile.
  ///
  /// In en, this message translates to:
  /// **'Choose file'**
  String get profilePickFile;

  /// No description provided for @profileFilePathHint.
  ///
  /// In en, this message translates to:
  /// **'File path (desktop)'**
  String get profileFilePathHint;

  /// No description provided for @profileUpload.
  ///
  /// In en, this message translates to:
  /// **'Upload'**
  String get profileUpload;

  /// No description provided for @profileUploadOk.
  ///
  /// In en, this message translates to:
  /// **'OK: uploaded'**
  String get profileUploadOk;

  /// No description provided for @profileSave.
  ///
  /// In en, this message translates to:
  /// **'Save'**
  String get profileSave;

  /// No description provided for @profileSavedOk.
  ///
  /// In en, this message translates to:
  /// **'OK: saved (core restarted)'**
  String get profileSavedOk;

  /// No description provided for @profileErrSave.
  ///
  /// In en, this message translates to:
  /// **'Save failed: {error}'**
  String profileErrSave(String error);

  /// No description provided for @profileErrUpload.
  ///
  /// In en, this message translates to:
  /// **'Upload failed: {error}'**
  String profileErrUpload(String error);

  /// No description provided for @profileErrCoreNotRunning.
  ///
  /// In en, this message translates to:
  /// **'Core not running.'**
  String get profileErrCoreNotRunning;

  /// No description provided for @profileFieldsTitle.
  ///
  /// In en, this message translates to:
  /// **'Profile fields'**
  String get profileFieldsTitle;

  /// No description provided for @profileFieldAdd.
  ///
  /// In en, this message translates to:
  /// **'Add field'**
  String get profileFieldAdd;

  /// No description provided for @profileFieldEdit.
  ///
  /// In en, this message translates to:
  /// **'Edit field'**
  String get profileFieldEdit;

  /// No description provided for @profileFieldName.
  ///
  /// In en, this message translates to:
  /// **'Name'**
  String get profileFieldName;

  /// No description provided for @profileFieldValue.
  ///
  /// In en, this message translates to:
  /// **'Value'**
  String get profileFieldValue;

  /// No description provided for @profileFieldNameEmpty.
  ///
  /// In en, this message translates to:
  /// **'(empty)'**
  String get profileFieldNameEmpty;

  /// No description provided for @privacyTitle.
  ///
  /// In en, this message translates to:
  /// **'Privacy'**
  String get privacyTitle;

  /// No description provided for @privacyHint.
  ///
  /// In en, this message translates to:
  /// **'Account privacy settings'**
  String get privacyHint;

  /// No description provided for @privacyLockedAccount.
  ///
  /// In en, this message translates to:
  /// **'Locked account (manual follow approval)'**
  String get privacyLockedAccount;

  /// No description provided for @privacyLockedAccountHint.
  ///
  /// In en, this message translates to:
  /// **'When enabled, incoming follow requests require approval.'**
  String get privacyLockedAccountHint;

  /// No description provided for @telemetrySectionTitle.
  ///
  /// In en, this message translates to:
  /// **'Diagnostics'**
  String get telemetrySectionTitle;

  /// No description provided for @telemetryEnabled.
  ///
  /// In en, this message translates to:
  /// **'Anonymous telemetry'**
  String get telemetryEnabled;

  /// No description provided for @telemetryEnabledHint.
  ///
  /// In en, this message translates to:
  /// **'Share minimal diagnostics (errors and performance) to improve stability. No personal content.'**
  String get telemetryEnabledHint;

  /// No description provided for @telemetryMonitoringEnabled.
  ///
  /// In en, this message translates to:
  /// **'Client monitoring (debug/staging)'**
  String get telemetryMonitoringEnabled;

  /// No description provided for @telemetryMonitoringHint.
  ///
  /// In en, this message translates to:
  /// **'Keep a local diagnostics log for troubleshooting.'**
  String get telemetryMonitoringHint;

  /// No description provided for @telemetryOpen.
  ///
  /// In en, this message translates to:
  /// **'Open diagnostics'**
  String get telemetryOpen;

  /// No description provided for @telemetryTitle.
  ///
  /// In en, this message translates to:
  /// **'Diagnostics'**
  String get telemetryTitle;

  /// No description provided for @telemetryRefresh.
  ///
  /// In en, this message translates to:
  /// **'Refresh'**
  String get telemetryRefresh;

  /// No description provided for @telemetryExport.
  ///
  /// In en, this message translates to:
  /// **'Export'**
  String get telemetryExport;

  /// No description provided for @telemetryExportEmpty.
  ///
  /// In en, this message translates to:
  /// **'No diagnostics to export'**
  String get telemetryExportEmpty;

  /// No description provided for @telemetryClear.
  ///
  /// In en, this message translates to:
  /// **'Clear'**
  String get telemetryClear;

  /// No description provided for @telemetryEmpty.
  ///
  /// In en, this message translates to:
  /// **'No diagnostics yet'**
  String get telemetryEmpty;

  /// No description provided for @updateAvailable.
  ///
  /// In en, this message translates to:
  /// **'Update available: {version}'**
  String updateAvailable(String version);

  /// No description provided for @updateInstall.
  ///
  /// In en, this message translates to:
  /// **'Install update'**
  String get updateInstall;

  /// No description provided for @updateDownloading.
  ///
  /// In en, this message translates to:
  /// **'Downloading...'**
  String get updateDownloading;

  /// No description provided for @updateFailed.
  ///
  /// In en, this message translates to:
  /// **'Update failed: {error}'**
  String updateFailed(String error);

  /// No description provided for @updateChangelog.
  ///
  /// In en, this message translates to:
  /// **'Changelog'**
  String get updateChangelog;

  /// No description provided for @updateDismiss.
  ///
  /// In en, this message translates to:
  /// **'Dismiss'**
  String get updateDismiss;

  /// No description provided for @updateOpenRelease.
  ///
  /// In en, this message translates to:
  /// **'Opening release page'**
  String get updateOpenRelease;

  /// No description provided for @updateManual.
  ///
  /// In en, this message translates to:
  /// **'Manual update'**
  String get updateManual;

  /// No description provided for @updateManualBody.
  ///
  /// In en, this message translates to:
  /// **'Run this command in a terminal:\\n{command}'**
  String updateManualBody(String command);

  /// No description provided for @securityTitle.
  ///
  /// In en, this message translates to:
  /// **'Security'**
  String get securityTitle;

  /// No description provided for @securityHint.
  ///
  /// In en, this message translates to:
  /// **'Tokens and internal endpoints'**
  String get securityHint;

  /// No description provided for @securityInternalToken.
  ///
  /// In en, this message translates to:
  /// **'Internal token'**
  String get securityInternalToken;

  /// No description provided for @securityInternalTokenHint.
  ///
  /// In en, this message translates to:
  /// **'Used to protect internal API endpoints'**
  String get securityInternalTokenHint;

  /// No description provided for @securityRegenerate.
  ///
  /// In en, this message translates to:
  /// **'Regenerate'**
  String get securityRegenerate;

  /// No description provided for @securityHintInternalEndpoints.
  ///
  /// In en, this message translates to:
  /// **'Keep this token private. If exposed, others on your network could call internal endpoints.'**
  String get securityHintInternalEndpoints;

  /// No description provided for @moderationTitle.
  ///
  /// In en, this message translates to:
  /// **'Moderation'**
  String get moderationTitle;

  /// No description provided for @moderationHintTitle.
  ///
  /// In en, this message translates to:
  /// **'Block lists and policies'**
  String get moderationHintTitle;

  /// No description provided for @moderationBlockedDomains.
  ///
  /// In en, this message translates to:
  /// **'Blocked domains'**
  String get moderationBlockedDomains;

  /// No description provided for @moderationBlockedDomainsHint.
  ///
  /// In en, this message translates to:
  /// **'One per line (example.com or *.example.com)'**
  String get moderationBlockedDomainsHint;

  /// No description provided for @moderationBlockedActors.
  ///
  /// In en, this message translates to:
  /// **'Blocked actors'**
  String get moderationBlockedActors;

  /// No description provided for @moderationBlockedActorsHint.
  ///
  /// In en, this message translates to:
  /// **'One actor URL per line'**
  String get moderationBlockedActorsHint;

  /// No description provided for @moderationHint.
  ///
  /// In en, this message translates to:
  /// **'Blocks apply to inbound and outbound interactions.'**
  String get moderationHint;

  /// No description provided for @networkingTitle.
  ///
  /// In en, this message translates to:
  /// **'Networking'**
  String get networkingTitle;

  /// No description provided for @networkingHintTitle.
  ///
  /// In en, this message translates to:
  /// **'Relay and AP relays'**
  String get networkingHintTitle;

  /// No description provided for @networkingRelay.
  ///
  /// In en, this message translates to:
  /// **'Relay public base URL'**
  String get networkingRelay;

  /// No description provided for @networkingRelayWs.
  ///
  /// In en, this message translates to:
  /// **'Relay WebSocket'**
  String get networkingRelayWs;

  /// No description provided for @networkingBind.
  ///
  /// In en, this message translates to:
  /// **'Local bind'**
  String get networkingBind;

  /// No description provided for @networkingRelaysTitle.
  ///
  /// In en, this message translates to:
  /// **'Relay discovery'**
  String get networkingRelaysTitle;

  /// No description provided for @networkingRelaysCount.
  ///
  /// In en, this message translates to:
  /// **'{count} relays known'**
  String networkingRelaysCount(int count);

  /// No description provided for @networkingRelaysEmpty.
  ///
  /// In en, this message translates to:
  /// **'No relays yet'**
  String get networkingRelaysEmpty;

  /// No description provided for @relayAdminTitle.
  ///
  /// In en, this message translates to:
  /// **'Relay admin'**
  String get relayAdminTitle;

  /// No description provided for @relayAdminHint.
  ///
  /// In en, this message translates to:
  /// **'Manage relay users and audit'**
  String get relayAdminHint;

  /// No description provided for @relayAdminRelayWsLabel.
  ///
  /// In en, this message translates to:
  /// **'Relay WS'**
  String get relayAdminRelayWsLabel;

  /// No description provided for @relayAdminTokenLabel.
  ///
  /// In en, this message translates to:
  /// **'Admin token'**
  String get relayAdminTokenLabel;

  /// No description provided for @relayAdminTokenHint.
  ///
  /// In en, this message translates to:
  /// **'Paste admin token'**
  String get relayAdminTokenHint;

  /// No description provided for @relayAdminTokenMissing.
  ///
  /// In en, this message translates to:
  /// **'Add an admin token to use relay admin.'**
  String get relayAdminTokenMissing;

  /// No description provided for @relayAdminUsers.
  ///
  /// In en, this message translates to:
  /// **'Users'**
  String get relayAdminUsers;

  /// No description provided for @relayAdminAudit.
  ///
  /// In en, this message translates to:
  /// **'Audit'**
  String get relayAdminAudit;

  /// No description provided for @relayAdminRegister.
  ///
  /// In en, this message translates to:
  /// **'Register'**
  String get relayAdminRegister;

  /// No description provided for @relayAdminRegisterHint.
  ///
  /// In en, this message translates to:
  /// **'username (e.g. alice)'**
  String get relayAdminRegisterHint;

  /// No description provided for @relayAdminUsername.
  ///
  /// In en, this message translates to:
  /// **'Username'**
  String get relayAdminUsername;

  /// No description provided for @relayAdminGenerateToken.
  ///
  /// In en, this message translates to:
  /// **'Generate token'**
  String get relayAdminGenerateToken;

  /// No description provided for @relayAdminRotate.
  ///
  /// In en, this message translates to:
  /// **'Rotate token'**
  String get relayAdminRotate;

  /// No description provided for @relayAdminEnable.
  ///
  /// In en, this message translates to:
  /// **'Enable'**
  String get relayAdminEnable;

  /// No description provided for @relayAdminDisable.
  ///
  /// In en, this message translates to:
  /// **'Disable'**
  String get relayAdminDisable;

  /// No description provided for @relayAdminDelete.
  ///
  /// In en, this message translates to:
  /// **'Delete'**
  String get relayAdminDelete;

  /// No description provided for @relayAdminAuditFailed.
  ///
  /// In en, this message translates to:
  /// **'Failed'**
  String get relayAdminAuditFailed;

  /// No description provided for @relayAdminUserEnabled.
  ///
  /// In en, this message translates to:
  /// **'Enabled'**
  String get relayAdminUserEnabled;

  /// No description provided for @relayAdminUserDisabled.
  ///
  /// In en, this message translates to:
  /// **'Disabled'**
  String get relayAdminUserDisabled;

  /// No description provided for @relayAdminUserSearchHint.
  ///
  /// In en, this message translates to:
  /// **'Search users…'**
  String get relayAdminUserSearchHint;

  /// No description provided for @relayAdminDeleteConfirm.
  ///
  /// In en, this message translates to:
  /// **'Delete user {username}?'**
  String relayAdminDeleteConfirm(String username);

  /// No description provided for @relayAdminAuditExport.
  ///
  /// In en, this message translates to:
  /// **'Export audit'**
  String get relayAdminAuditExport;

  /// No description provided for @relayAdminAuditExported.
  ///
  /// In en, this message translates to:
  /// **'Audit exported: {path}'**
  String relayAdminAuditExported(String path);

  /// No description provided for @relayAdminAuditSearchHint.
  ///
  /// In en, this message translates to:
  /// **'Search audit…'**
  String get relayAdminAuditSearchHint;

  /// No description provided for @relayAdminAuditFailedOnly.
  ///
  /// In en, this message translates to:
  /// **'Failed only'**
  String get relayAdminAuditFailedOnly;

  /// No description provided for @relayAdminAuditReverse.
  ///
  /// In en, this message translates to:
  /// **'Reverse order'**
  String get relayAdminAuditReverse;

  /// No description provided for @relayAdminUsersCount.
  ///
  /// In en, this message translates to:
  /// **'Users: {count}'**
  String relayAdminUsersCount(int count);

  /// No description provided for @relayAdminUsersDisabledCount.
  ///
  /// In en, this message translates to:
  /// **'Disabled: {count}'**
  String relayAdminUsersDisabledCount(int count);

  /// No description provided for @relayAdminAuditLast.
  ///
  /// In en, this message translates to:
  /// **'Last audit'**
  String get relayAdminAuditLast;

  /// No description provided for @networkingRelaysError.
  ///
  /// In en, this message translates to:
  /// **'Relay sync failed'**
  String get networkingRelaysError;

  /// No description provided for @networkingApRelays.
  ///
  /// In en, this message translates to:
  /// **'ActivityPub relays'**
  String get networkingApRelays;

  /// No description provided for @networkingApRelaysEmpty.
  ///
  /// In en, this message translates to:
  /// **'No AP relays configured'**
  String get networkingApRelaysEmpty;

  /// No description provided for @networkingEditAccount.
  ///
  /// In en, this message translates to:
  /// **'Edit account/networking'**
  String get networkingEditAccount;

  /// No description provided for @networkingHint.
  ///
  /// In en, this message translates to:
  /// **'Some changes require restarting the core.'**
  String get networkingHint;

  /// No description provided for @p2pSectionTitle.
  ///
  /// In en, this message translates to:
  /// **'P2P delivery'**
  String get p2pSectionTitle;

  /// No description provided for @p2pDeliveryModeLabel.
  ///
  /// In en, this message translates to:
  /// **'Delivery mode'**
  String get p2pDeliveryModeLabel;

  /// No description provided for @p2pDeliveryModeRelay.
  ///
  /// In en, this message translates to:
  /// **'P2P first, fallback to relay'**
  String get p2pDeliveryModeRelay;

  /// No description provided for @p2pDeliveryModeP2POnly.
  ///
  /// In en, this message translates to:
  /// **'P2P only (no relay)'**
  String get p2pDeliveryModeP2POnly;

  /// No description provided for @p2pRelayFallbackLabel.
  ///
  /// In en, this message translates to:
  /// **'Relay fallback delay (seconds)'**
  String get p2pRelayFallbackLabel;

  /// No description provided for @p2pRelayFallbackHint.
  ///
  /// In en, this message translates to:
  /// **'Wait before using relay (default: 5)'**
  String get p2pRelayFallbackHint;

  /// No description provided for @p2pCacheTtlLabel.
  ///
  /// In en, this message translates to:
  /// **'Mailbox cache TTL (seconds)'**
  String get p2pCacheTtlLabel;

  /// No description provided for @p2pCacheTtlHint.
  ///
  /// In en, this message translates to:
  /// **'Store-and-forward TTL (default: 604800)'**
  String get p2pCacheTtlHint;

  /// No description provided for @backupTitle.
  ///
  /// In en, this message translates to:
  /// **'Backup'**
  String get backupTitle;

  /// No description provided for @backupHint.
  ///
  /// In en, this message translates to:
  /// **'Export/import your Fedi3 profile and settings'**
  String get backupHint;

  /// No description provided for @backupExportTitle.
  ///
  /// In en, this message translates to:
  /// **'Export'**
  String get backupExportTitle;

  /// No description provided for @backupExportHint.
  ///
  /// In en, this message translates to:
  /// **'Creates a JSON backup (config + UI prefs).'**
  String get backupExportHint;

  /// No description provided for @backupExportSave.
  ///
  /// In en, this message translates to:
  /// **'Save backup file'**
  String get backupExportSave;

  /// No description provided for @backupExportCopy.
  ///
  /// In en, this message translates to:
  /// **'Copy backup JSON'**
  String get backupExportCopy;

  /// No description provided for @backupExportOk.
  ///
  /// In en, this message translates to:
  /// **'OK: copied to clipboard'**
  String get backupExportOk;

  /// No description provided for @backupExportSaved.
  ///
  /// In en, this message translates to:
  /// **'OK: saved backup file'**
  String get backupExportSaved;

  /// No description provided for @backupImportTitle.
  ///
  /// In en, this message translates to:
  /// **'Import'**
  String get backupImportTitle;

  /// No description provided for @backupImportHint.
  ///
  /// In en, this message translates to:
  /// **'Paste a previously exported JSON backup here.'**
  String get backupImportHint;

  /// No description provided for @backupImportApply.
  ///
  /// In en, this message translates to:
  /// **'Import now'**
  String get backupImportApply;

  /// No description provided for @backupImportFile.
  ///
  /// In en, this message translates to:
  /// **'Import from file'**
  String get backupImportFile;

  /// No description provided for @backupImportOk.
  ///
  /// In en, this message translates to:
  /// **'OK: imported (core restarted)'**
  String get backupImportOk;

  /// No description provided for @backupCloudTitle.
  ///
  /// In en, this message translates to:
  /// **'Cloud backup'**
  String get backupCloudTitle;

  /// No description provided for @backupCloudHint.
  ///
  /// In en, this message translates to:
  /// **'Encrypted backup stored on your relay (or S3) for fast device sync.'**
  String get backupCloudHint;

  /// No description provided for @backupCloudUpload.
  ///
  /// In en, this message translates to:
  /// **'Upload to relay'**
  String get backupCloudUpload;

  /// No description provided for @backupCloudDownload.
  ///
  /// In en, this message translates to:
  /// **'Restore from relay'**
  String get backupCloudDownload;

  /// No description provided for @backupCloudUploadOk.
  ///
  /// In en, this message translates to:
  /// **'OK: backup uploaded'**
  String get backupCloudUploadOk;

  /// No description provided for @backupCloudDownloadOk.
  ///
  /// In en, this message translates to:
  /// **'OK: backup restored'**
  String get backupCloudDownloadOk;

  /// No description provided for @backupErr.
  ///
  /// In en, this message translates to:
  /// **'Backup error: {error}'**
  String backupErr(String error);

  /// No description provided for @statusRelay.
  ///
  /// In en, this message translates to:
  /// **'Relay'**
  String get statusRelay;

  /// No description provided for @statusMailbox.
  ///
  /// In en, this message translates to:
  /// **'Mailbox'**
  String get statusMailbox;

  /// No description provided for @statusRelayRtt.
  ///
  /// In en, this message translates to:
  /// **'R'**
  String get statusRelayRtt;

  /// No description provided for @statusMailboxRtt.
  ///
  /// In en, this message translates to:
  /// **'M'**
  String get statusMailboxRtt;

  /// No description provided for @statusRelayTraffic.
  ///
  /// In en, this message translates to:
  /// **'R ↑/↓'**
  String get statusRelayTraffic;

  /// No description provided for @statusMailboxTraffic.
  ///
  /// In en, this message translates to:
  /// **'M ↑/↓'**
  String get statusMailboxTraffic;

  /// No description provided for @statusCoreStoppedShort.
  ///
  /// In en, this message translates to:
  /// **'core off'**
  String get statusCoreStoppedShort;

  /// No description provided for @statusUnknownShort.
  ///
  /// In en, this message translates to:
  /// **'?'**
  String get statusUnknownShort;

  /// No description provided for @statusConnectedShort.
  ///
  /// In en, this message translates to:
  /// **'on'**
  String get statusConnectedShort;

  /// No description provided for @statusDisconnectedShort.
  ///
  /// In en, this message translates to:
  /// **'off'**
  String get statusDisconnectedShort;

  /// No description provided for @statusNoPeersShort.
  ///
  /// In en, this message translates to:
  /// **'0/0'**
  String get statusNoPeersShort;

  /// No description provided for @settingsOk.
  ///
  /// In en, this message translates to:
  /// **'OK'**
  String get settingsOk;

  /// No description provided for @settingsErr.
  ///
  /// In en, this message translates to:
  /// **'ERR: {error}'**
  String settingsErr(String error);

  /// No description provided for @relaysTitle.
  ///
  /// In en, this message translates to:
  /// **'Relays'**
  String get relaysTitle;

  /// No description provided for @relaysCurrent.
  ///
  /// In en, this message translates to:
  /// **'Current relay'**
  String get relaysCurrent;

  /// No description provided for @relaysTelemetry.
  ///
  /// In en, this message translates to:
  /// **'Telemetry'**
  String get relaysTelemetry;

  /// No description provided for @relaysKnown.
  ///
  /// In en, this message translates to:
  /// **'Known relays'**
  String get relaysKnown;

  /// No description provided for @relaysLastSeen.
  ///
  /// In en, this message translates to:
  /// **'last_seen={ms}'**
  String relaysLastSeen(String ms);

  /// No description provided for @relaysPeersTitle.
  ///
  /// In en, this message translates to:
  /// **'Known peers'**
  String get relaysPeersTitle;

  /// No description provided for @relaysPeersSearchHint.
  ///
  /// In en, this message translates to:
  /// **'Search peers'**
  String get relaysPeersSearchHint;

  /// No description provided for @relaysPeersEmpty.
  ///
  /// In en, this message translates to:
  /// **'No peers found'**
  String get relaysPeersEmpty;

  /// No description provided for @relaysRecommended.
  ///
  /// In en, this message translates to:
  /// **'Recommended'**
  String get relaysRecommended;

  /// No description provided for @relaysLatency.
  ///
  /// In en, this message translates to:
  /// **'Latency: {ms} ms'**
  String relaysLatency(int ms);

  /// No description provided for @relaysCoverageTitle.
  ///
  /// In en, this message translates to:
  /// **'Search coverage'**
  String get relaysCoverageTitle;

  /// No description provided for @relaysCoverageUsers.
  ///
  /// In en, this message translates to:
  /// **'{indexed}/{total} users indexed'**
  String relaysCoverageUsers(int indexed, int total);

  /// No description provided for @relaysCoverageLast.
  ///
  /// In en, this message translates to:
  /// **'Last index: {ms}'**
  String relaysCoverageLast(String ms);

  /// No description provided for @onboardingTitle.
  ///
  /// In en, this message translates to:
  /// **'Fedi3 setup'**
  String get onboardingTitle;

  /// No description provided for @onboardingIntro.
  ///
  /// In en, this message translates to:
  /// **'Create your local instance and connect it to a relay (for legacy compatibility).'**
  String get onboardingIntro;

  /// No description provided for @onboardingUsername.
  ///
  /// In en, this message translates to:
  /// **'Username'**
  String get onboardingUsername;

  /// No description provided for @onboardingDomain.
  ///
  /// In en, this message translates to:
  /// **'Domain (handle: user@domain)'**
  String get onboardingDomain;

  /// No description provided for @onboardingRelayPublicUrl.
  ///
  /// In en, this message translates to:
  /// **'Relay public URL (https://relay... or http://127.0.0.1:8787)'**
  String get onboardingRelayPublicUrl;

  /// No description provided for @onboardingRelayWs.
  ///
  /// In en, this message translates to:
  /// **'Relay WS (wss://... or ws://127.0.0.1:8787)'**
  String get onboardingRelayWs;

  /// No description provided for @onboardingRelayToken.
  ///
  /// In en, this message translates to:
  /// **'Relay token'**
  String get onboardingRelayToken;

  /// No description provided for @onboardingBind.
  ///
  /// In en, this message translates to:
  /// **'Local bind (host:port)'**
  String get onboardingBind;

  /// No description provided for @onboardingInternalToken.
  ///
  /// In en, this message translates to:
  /// **'Internal token (UI ↔ core)'**
  String get onboardingInternalToken;

  /// No description provided for @onboardingSave.
  ///
  /// In en, this message translates to:
  /// **'Save & open app'**
  String get onboardingSave;

  /// No description provided for @onboardingRelayTokenTooShort.
  ///
  /// In en, this message translates to:
  /// **'Relay token too short (min 16 characters).'**
  String get onboardingRelayTokenTooShort;

  /// No description provided for @editAccountTitle.
  ///
  /// In en, this message translates to:
  /// **'Edit account'**
  String get editAccountTitle;

  /// No description provided for @editAccountSave.
  ///
  /// In en, this message translates to:
  /// **'Save'**
  String get editAccountSave;

  /// No description provided for @editAccountCoreRunningWarning.
  ///
  /// In en, this message translates to:
  /// **'Core is running: it will be stopped automatically before saving to avoid inconsistencies.'**
  String get editAccountCoreRunningWarning;

  /// No description provided for @editAccountRelayPublicUrl.
  ///
  /// In en, this message translates to:
  /// **'Relay public URL (https://relay.fedi3.com)'**
  String get editAccountRelayPublicUrl;

  /// No description provided for @editAccountRelayWs.
  ///
  /// In en, this message translates to:
  /// **'Relay WS (wss://relay.fedi3.com)'**
  String get editAccountRelayWs;

  /// No description provided for @editAccountRegenerateInternal.
  ///
  /// In en, this message translates to:
  /// **'Regenerate internal token'**
  String get editAccountRegenerateInternal;

  /// No description provided for @devCoreTitle.
  ///
  /// In en, this message translates to:
  /// **'Dev core controls'**
  String get devCoreTitle;

  /// No description provided for @devCoreConfigSaved.
  ///
  /// In en, this message translates to:
  /// **'Config (saved)'**
  String get devCoreConfigSaved;

  /// No description provided for @devCoreStart.
  ///
  /// In en, this message translates to:
  /// **'Start'**
  String get devCoreStart;

  /// No description provided for @devCoreStop.
  ///
  /// In en, this message translates to:
  /// **'Stop'**
  String get devCoreStop;

  /// No description provided for @devCoreFetchMigration.
  ///
  /// In en, this message translates to:
  /// **'Fetch migration status'**
  String get devCoreFetchMigration;

  /// No description provided for @devCoreVersion.
  ///
  /// In en, this message translates to:
  /// **'Core version: {version}'**
  String devCoreVersion(String version);

  /// No description provided for @devCoreNotLoaded.
  ///
  /// In en, this message translates to:
  /// **'Core not loaded: {error}'**
  String devCoreNotLoaded(String error);

  /// No description provided for @notificationsTitle.
  ///
  /// In en, this message translates to:
  /// **'Notifications'**
  String get notificationsTitle;

  /// No description provided for @notificationsEmpty.
  ///
  /// In en, this message translates to:
  /// **'No notifications'**
  String get notificationsEmpty;

  /// No description provided for @notificationsCoreNotRunning.
  ///
  /// In en, this message translates to:
  /// **'Core not running.'**
  String get notificationsCoreNotRunning;

  /// No description provided for @notificationsGeneric.
  ///
  /// In en, this message translates to:
  /// **'Notification'**
  String get notificationsGeneric;

  /// No description provided for @notificationsFollow.
  ///
  /// In en, this message translates to:
  /// **'New follower'**
  String get notificationsFollow;

  /// No description provided for @notificationsFollowAccepted.
  ///
  /// In en, this message translates to:
  /// **'Follow accepted'**
  String get notificationsFollowAccepted;

  /// No description provided for @notificationsFollowRejected.
  ///
  /// In en, this message translates to:
  /// **'Follow rejected'**
  String get notificationsFollowRejected;

  /// No description provided for @notificationsLike.
  ///
  /// In en, this message translates to:
  /// **'Liked your post'**
  String get notificationsLike;

  /// No description provided for @notificationsReact.
  ///
  /// In en, this message translates to:
  /// **'Reacted to your post'**
  String get notificationsReact;

  /// No description provided for @notificationsBoost.
  ///
  /// In en, this message translates to:
  /// **'Boosted your post'**
  String get notificationsBoost;

  /// No description provided for @notificationsMentionOrReply.
  ///
  /// In en, this message translates to:
  /// **'Mention / reply'**
  String get notificationsMentionOrReply;

  /// No description provided for @notificationsNewActivity.
  ///
  /// In en, this message translates to:
  /// **'New activity'**
  String get notificationsNewActivity;

  /// No description provided for @uiSettingsTitle.
  ///
  /// In en, this message translates to:
  /// **'Appearance & language'**
  String get uiSettingsTitle;

  /// No description provided for @uiSettingsHint.
  ///
  /// In en, this message translates to:
  /// **'Theme, language, density, font size'**
  String get uiSettingsHint;

  /// No description provided for @uiLanguage.
  ///
  /// In en, this message translates to:
  /// **'Language'**
  String get uiLanguage;

  /// No description provided for @uiLanguageSystem.
  ///
  /// In en, this message translates to:
  /// **'System default'**
  String get uiLanguageSystem;

  /// No description provided for @uiTheme.
  ///
  /// In en, this message translates to:
  /// **'Theme'**
  String get uiTheme;

  /// No description provided for @uiThemeSystem.
  ///
  /// In en, this message translates to:
  /// **'System'**
  String get uiThemeSystem;

  /// No description provided for @uiThemeLight.
  ///
  /// In en, this message translates to:
  /// **'Light'**
  String get uiThemeLight;

  /// No description provided for @uiThemeDark.
  ///
  /// In en, this message translates to:
  /// **'Dark'**
  String get uiThemeDark;

  /// No description provided for @uiDensity.
  ///
  /// In en, this message translates to:
  /// **'Density'**
  String get uiDensity;

  /// No description provided for @uiDensityNormal.
  ///
  /// In en, this message translates to:
  /// **'Normal'**
  String get uiDensityNormal;

  /// No description provided for @uiDensityCompact.
  ///
  /// In en, this message translates to:
  /// **'Compact'**
  String get uiDensityCompact;

  /// No description provided for @uiAccent.
  ///
  /// In en, this message translates to:
  /// **'Accent color'**
  String get uiAccent;

  /// No description provided for @uiFontSize.
  ///
  /// In en, this message translates to:
  /// **'Font size'**
  String get uiFontSize;

  /// No description provided for @uiFontSizeHint.
  ///
  /// In en, this message translates to:
  /// **'Affects the whole app'**
  String get uiFontSizeHint;

  /// No description provided for @gifSettingsTitle.
  ///
  /// In en, this message translates to:
  /// **'GIFs'**
  String get gifSettingsTitle;

  /// No description provided for @gifSettingsHint.
  ///
  /// In en, this message translates to:
  /// **'Giphy API key'**
  String get gifSettingsHint;

  /// No description provided for @gifProviderLabel.
  ///
  /// In en, this message translates to:
  /// **'Provider'**
  String get gifProviderLabel;

  /// No description provided for @gifProviderHint.
  ///
  /// In en, this message translates to:
  /// **'Use your own API key to enable GIF search.'**
  String get gifProviderHint;

  /// No description provided for @gifProviderTenor.
  ///
  /// In en, this message translates to:
  /// **'Tenor'**
  String get gifProviderTenor;

  /// No description provided for @gifProviderGiphy.
  ///
  /// In en, this message translates to:
  /// **'Giphy'**
  String get gifProviderGiphy;

  /// No description provided for @gifApiKeyLabel.
  ///
  /// In en, this message translates to:
  /// **'API key'**
  String get gifApiKeyLabel;

  /// No description provided for @gifApiKeyHint.
  ///
  /// In en, this message translates to:
  /// **'Paste your Giphy API key'**
  String get gifApiKeyHint;

  /// No description provided for @gifSettingsDefaultHint.
  ///
  /// In en, this message translates to:
  /// **'You can use the default Giphy key for now.'**
  String get gifSettingsDefaultHint;

  /// No description provided for @gifSettingsUseDefault.
  ///
  /// In en, this message translates to:
  /// **'Use default key'**
  String get gifSettingsUseDefault;

  /// No description provided for @composeContentWarningTitle.
  ///
  /// In en, this message translates to:
  /// **'Content warning'**
  String get composeContentWarningTitle;

  /// No description provided for @composeContentWarningHint.
  ///
  /// In en, this message translates to:
  /// **'Hide content behind a warning'**
  String get composeContentWarningHint;

  /// No description provided for @composeContentWarningTextLabel.
  ///
  /// In en, this message translates to:
  /// **'Warning text'**
  String get composeContentWarningTextLabel;

  /// No description provided for @composeSensitiveMediaTitle.
  ///
  /// In en, this message translates to:
  /// **'Sensitive media'**
  String get composeSensitiveMediaTitle;

  /// No description provided for @composeSensitiveMediaHint.
  ///
  /// In en, this message translates to:
  /// **'Mark media as sensitive'**
  String get composeSensitiveMediaHint;

  /// No description provided for @composeEmojiButton.
  ///
  /// In en, this message translates to:
  /// **'Emoji'**
  String get composeEmojiButton;

  /// No description provided for @composeMfmCheatsheet.
  ///
  /// In en, this message translates to:
  /// **'MFM cheatsheet'**
  String get composeMfmCheatsheet;

  /// No description provided for @composeMfmCheatsheetTitle.
  ///
  /// In en, this message translates to:
  /// **'MFM cheatsheet'**
  String get composeMfmCheatsheetTitle;

  /// No description provided for @composeMfmCheatsheetBody.
  ///
  /// In en, this message translates to:
  /// **'**bold** -> bold\n*italic* -> italic\n~~strike~~ -> strikethrough\n`code` -> inline code\n```code``` -> code block\n> quote -> quote block\n[title](https://example.com) -> link\n#tag -> hashtag\n@user@domain -> mention\n:emoji: -> custom emoji\nLine breaks -> new line'**
  String get composeMfmCheatsheetBody;

  /// No description provided for @close.
  ///
  /// In en, this message translates to:
  /// **'Close'**
  String get close;

  /// No description provided for @composeVisibilityTitle.
  ///
  /// In en, this message translates to:
  /// **'Visibility'**
  String get composeVisibilityTitle;

  /// No description provided for @composeVisibilityHint.
  ///
  /// In en, this message translates to:
  /// **'Who can see this post'**
  String get composeVisibilityHint;

  /// No description provided for @composeVisibilityPublic.
  ///
  /// In en, this message translates to:
  /// **'Public'**
  String get composeVisibilityPublic;

  /// No description provided for @composeVisibilityHome.
  ///
  /// In en, this message translates to:
  /// **'Home'**
  String get composeVisibilityHome;

  /// No description provided for @composeVisibilityFollowers.
  ///
  /// In en, this message translates to:
  /// **'Followers only'**
  String get composeVisibilityFollowers;

  /// No description provided for @composeVisibilityDirect.
  ///
  /// In en, this message translates to:
  /// **'Direct'**
  String get composeVisibilityDirect;

  /// No description provided for @composeVisibilityDirectLabel.
  ///
  /// In en, this message translates to:
  /// **'Direct recipient'**
  String get composeVisibilityDirectLabel;

  /// No description provided for @composeVisibilityDirectHint.
  ///
  /// In en, this message translates to:
  /// **'@user@host or actor URL'**
  String get composeVisibilityDirectHint;

  /// No description provided for @composeVisibilityDirectMissing.
  ///
  /// In en, this message translates to:
  /// **'Direct recipient is required'**
  String get composeVisibilityDirectMissing;

  /// No description provided for @composeExpand.
  ///
  /// In en, this message translates to:
  /// **'Expand'**
  String get composeExpand;

  /// No description provided for @composeExpandTitle.
  ///
  /// In en, this message translates to:
  /// **'Composer'**
  String get composeExpandTitle;

  /// No description provided for @uiEmojiPaletteTitle.
  ///
  /// In en, this message translates to:
  /// **'Emoji palette'**
  String get uiEmojiPaletteTitle;

  /// No description provided for @uiEmojiPaletteHint.
  ///
  /// In en, this message translates to:
  /// **'Edit quick emojis'**
  String get uiEmojiPaletteHint;

  /// No description provided for @emojiPaletteAddLabel.
  ///
  /// In en, this message translates to:
  /// **'Add emoji'**
  String get emojiPaletteAddLabel;

  /// No description provided for @emojiPaletteAddHint.
  ///
  /// In en, this message translates to:
  /// **'😀 or :shortcode:'**
  String get emojiPaletteAddHint;

  /// No description provided for @emojiPaletteAddButton.
  ///
  /// In en, this message translates to:
  /// **'Add'**
  String get emojiPaletteAddButton;

  /// No description provided for @emojiPaletteEmpty.
  ///
  /// In en, this message translates to:
  /// **'No emojis yet.'**
  String get emojiPaletteEmpty;

  /// No description provided for @emojiPickerTitle.
  ///
  /// In en, this message translates to:
  /// **'Emoji'**
  String get emojiPickerTitle;

  /// No description provided for @emojiPickerClose.
  ///
  /// In en, this message translates to:
  /// **'Close'**
  String get emojiPickerClose;

  /// No description provided for @emojiPickerSearchLabel.
  ///
  /// In en, this message translates to:
  /// **'Search'**
  String get emojiPickerSearchLabel;

  /// No description provided for @emojiPickerSearchHint.
  ///
  /// In en, this message translates to:
  /// **'Emoji or :shortcode:'**
  String get emojiPickerSearchHint;

  /// No description provided for @emojiPickerPalette.
  ///
  /// In en, this message translates to:
  /// **'Palette'**
  String get emojiPickerPalette;

  /// No description provided for @emojiPickerRecent.
  ///
  /// In en, this message translates to:
  /// **'Recent'**
  String get emojiPickerRecent;

  /// No description provided for @emojiPickerCommon.
  ///
  /// In en, this message translates to:
  /// **'Common'**
  String get emojiPickerCommon;

  /// No description provided for @emojiPickerCustom.
  ///
  /// In en, this message translates to:
  /// **'Custom emojis'**
  String get emojiPickerCustom;

  /// No description provided for @reactionPickerTitle.
  ///
  /// In en, this message translates to:
  /// **'Reactions'**
  String get reactionPickerTitle;

  /// No description provided for @reactionPickerClose.
  ///
  /// In en, this message translates to:
  /// **'Close'**
  String get reactionPickerClose;

  /// No description provided for @reactionPickerSearchLabel.
  ///
  /// In en, this message translates to:
  /// **'Search or custom'**
  String get reactionPickerSearchLabel;

  /// No description provided for @reactionPickerSearchHint.
  ///
  /// In en, this message translates to:
  /// **'Emoji or :shortcode:'**
  String get reactionPickerSearchHint;

  /// No description provided for @reactionPickerRecent.
  ///
  /// In en, this message translates to:
  /// **'Recent'**
  String get reactionPickerRecent;

  /// No description provided for @reactionPickerCommon.
  ///
  /// In en, this message translates to:
  /// **'Common'**
  String get reactionPickerCommon;

  /// No description provided for @reactionPickerNoteEmojis.
  ///
  /// In en, this message translates to:
  /// **'This note emojis'**
  String get reactionPickerNoteEmojis;

  /// No description provided for @reactionPickerGlobalEmojis.
  ///
  /// In en, this message translates to:
  /// **'Global custom emojis'**
  String get reactionPickerGlobalEmojis;

  /// No description provided for @uiEmojiPickerTitle.
  ///
  /// In en, this message translates to:
  /// **'Emoji picker'**
  String get uiEmojiPickerTitle;

  /// No description provided for @uiEmojiPickerSizeLabel.
  ///
  /// In en, this message translates to:
  /// **'Size'**
  String get uiEmojiPickerSizeLabel;

  /// No description provided for @uiEmojiPickerColumnsLabel.
  ///
  /// In en, this message translates to:
  /// **'Columns'**
  String get uiEmojiPickerColumnsLabel;

  /// No description provided for @uiEmojiPickerStyleLabel.
  ///
  /// In en, this message translates to:
  /// **'Custom emoji style'**
  String get uiEmojiPickerStyleLabel;

  /// No description provided for @uiEmojiPickerStyleImage.
  ///
  /// In en, this message translates to:
  /// **'Image'**
  String get uiEmojiPickerStyleImage;

  /// No description provided for @uiEmojiPickerStyleText.
  ///
  /// In en, this message translates to:
  /// **'Text'**
  String get uiEmojiPickerStyleText;

  /// No description provided for @uiEmojiPickerPresetLabel.
  ///
  /// In en, this message translates to:
  /// **'Preset'**
  String get uiEmojiPickerPresetLabel;

  /// No description provided for @uiEmojiPickerPresetCompact.
  ///
  /// In en, this message translates to:
  /// **'Compact'**
  String get uiEmojiPickerPresetCompact;

  /// No description provided for @uiEmojiPickerPresetComfort.
  ///
  /// In en, this message translates to:
  /// **'Comfort'**
  String get uiEmojiPickerPresetComfort;

  /// No description provided for @uiEmojiPickerPresetLarge.
  ///
  /// In en, this message translates to:
  /// **'Large'**
  String get uiEmojiPickerPresetLarge;

  /// No description provided for @uiEmojiPickerPreviewLabel.
  ///
  /// In en, this message translates to:
  /// **'Preview'**
  String get uiEmojiPickerPreviewLabel;

  /// No description provided for @uiNotificationsTitle.
  ///
  /// In en, this message translates to:
  /// **'Notifications'**
  String get uiNotificationsTitle;

  /// No description provided for @uiNotificationsChat.
  ///
  /// In en, this message translates to:
  /// **'Chat messages'**
  String get uiNotificationsChat;

  /// No description provided for @uiNotificationsDirect.
  ///
  /// In en, this message translates to:
  /// **'Direct interactions'**
  String get uiNotificationsDirect;
}

class _AppLocalizationsDelegate
    extends LocalizationsDelegate<AppLocalizations> {
  const _AppLocalizationsDelegate();

  @override
  Future<AppLocalizations> load(Locale locale) {
    return SynchronousFuture<AppLocalizations>(lookupAppLocalizations(locale));
  }

  @override
  bool isSupported(Locale locale) =>
      <String>['en', 'it'].contains(locale.languageCode);

  @override
  bool shouldReload(_AppLocalizationsDelegate old) => false;
}

AppLocalizations lookupAppLocalizations(Locale locale) {
  // Lookup logic when only language code is specified.
  switch (locale.languageCode) {
    case 'en':
      return AppLocalizationsEn();
    case 'it':
      return AppLocalizationsIt();
  }

  throw FlutterError(
      'AppLocalizations.delegate failed to load unsupported locale "$locale". This is likely '
      'an issue with the localizations generation tool. Please file an issue '
      'on GitHub with a reproducible sample app and the gen-l10n configuration '
      'that was used.');
}
