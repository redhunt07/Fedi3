# Client 0.11.1 verification — 2026-11-08

Release: [v0.11.1](https://github.com/redhunt07/Fedi3/releases/tag/v0.11.1).
Flutter version: `0.11.1+4`.

## Reported failures

Two Android clients on the public relay could not ring each other. Neither side
received a call notification, chat delivery looked delayed, and the chat
composer was covered by the Android navigation bar.

## Root causes

1. **Notifications were rejected before being posted.** Release resource
   shrinking removed the `res/raw` notification sounds because they are only
   referenced by name at runtime. `flutter_local_notifications` then threw
   `PlatformException(invalid_sound, The resource fedi3_call could not be
   found...)` and no call or chat notification appeared. `res/raw/keep.xml`
   now pins the three sounds.
2. **The headless keep-alive engine had no configuration.** The service
   registered generated plugins a second time, which replaced plugin instances
   and made `flutter_secure_storage` fail with `Could not decrypt key`. The
   engine therefore loaded no config and returned before polling calls. The
   duplicate registration was removed and the engine now falls back to an
   app-private `background_config.json` cache when the Keystore is unavailable.
3. **Call signaling was too fragile.** The shell polled calls with an idle
   backoff that could delay an invite by up to 15 s, and a single lost invite or
   accept left both peers waiting forever. Invites and accepts are now retried
   every 3 s while ringing, and the shell polls every 3 s.
4. **Queued chat recovered slowly.** The retry worker ran every 20 s with a
   20 s initial backoff. It now runs every 5 s with a 5 s initial backoff and a
   one-minute cap, which shortens recovery for messages and call signals.
5. **Android insets.** The chat composer and call controls now sit inside a
   bottom `SafeArea`, above the navigation bar and gesture pill.
6. **Duplicate Hero tags.** Pages kept mounted in the shell exposed several
   default floating-action-button Hero tags at once; `HeroMode` now limits hero
   collection to the visible page.
7. **Android screen sharing removed.** The Android call UI no longer shows the
   screen-share control, so audio/video calls do not start the MediaProjection
   foreground service. Desktop calls keep screen sharing.
8. **Stale incoming-call prompts.** An unanswered prompt stayed on screen after
   the caller stopped ringing. Prompts now expire after 60 s, and a `leave` for
   the ringing call closes the open prompt. The caller also ends the call after
   its one-minute invite retry window.

## Verification performed

- `flutter analyze`: no issues.
- Flutter test suite: 201 tests passed.
- Rust: `cargo test -p fedi3_core --test semantics --test db_queue --test
  p2p_chat` passed, including encrypted chat, receipts, presence and call
  signaling between two real cores.
- Android 13 emulator (`aliceemu`) and Android 11 emulator (`bobemu`) against
  `https://fedi3.redhunt07.ovh`:
  - A call started from bobemu displayed the incoming-call dialog on aliceemu;
    accepting it connected both sides and the duration counter advanced.
  - With bobemu backgrounded, the keep-alive service posted notification id
    1004 in the `fedi3_call_v2` channel (importance 5, category call); the
    heads-up notification showed "Incoming call — aliceemu", and tapping it
    opened the call prompt. Accepting connected the call.
  - A chat message sent from bobemu appeared on aliceemu within the polling
    interval and showed as "just now".
  - The chat composer and the call controls render above the system navigation
    bar on both Android versions.
- The release APK contains `res/raw` copies of the three notification sounds
  after the `keep.xml` fix.
- Xiaomi 2201123G (Android 15, TIM 5G, account `redhunt07` on
  `https://fedi3.redhunt07.ovh`) against the Android 11 emulator (`bobemu`):
  - A chat message sent from the phone over 5G arrived in bobemu's core and
    produced background chat notification id 1001 in `fedi3_chat_v2`.
  - An audio call from the phone over 5G rang bobemu while the app was closed:
    notification id 1004 in `fedi3_call_v2`, heads-up "Incoming call —
    redhunt07". Tapping it and accepting connected both sides, with the
    duration counter advancing to 00:08.
  - The phone's call UI no longer shows the screen-share control.
  - The reverse direction was exercised too: with the phone on the launcher
    (app closed), a call from bobemu posted notification id 1004 in
    `fedi3_call_v2`; the heads-up showed "Incoming call — bobemu", the
    background engine logged the ring, and accepting connected both sides
    (duration 00:10). Android 15/MIUI reported
    `FSI_REQUESTED_BUT_DENIED` for the full-screen intent, but the heads-up
    notification still surfaced.
- Unanswered-call expiry: a call from aliceemu to bobemu left unanswered
  closed both sides after 60 s; the caller returned to the chat and the
  callee's dialog disappeared, with no stale prompt.

## Limits

- The phone and emulator use the same public relay. The cross-relay path
  (`fedi3.redhunt07.ovh` ↔ `fedi3.palermo.nohost.me`) relies on the same relay
  HTTP forwarding and on the new retries; a call from the phone to `panormus`
  rang but was not answered during this pass.
- Audio was not judged by ear on the emulators; the notification channel and
  call connection state were verified instead.
- The real phone is Android 15 with MIUI. Its full-screen-intent request was
  denied by the system, so the incoming call relied on the heads-up
  notification; the call still connected.
- APKs still use the local debug signing key because `android/key.properties`
  is not configured. Existing profiles and keyring data are preserved.
