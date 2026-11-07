# FEDI3 app guide

FEDI3 is made of two parts that run on your machine:

- the Flutter **app**, which is the interface;
- the local Rust **core**, which handles identity, cryptography, cache and federation.

The core listens on `127.0.0.1` and is protected by an automatically generated internal token. The relay is optional: without a relay you can still use P2P and chat, but you are not reachable from traditional fediverse instances.

## First start

1. Open the app. If no configuration exists, the welcome screen is shown.
2. Fill in:
   - **Public domain**: the hostname through which you want to be reachable (e.g. `my.name`). It must point to your instance or to the relay you use.
   - **Username**: only letters, digits and `_`, from 1 to 64 characters. It cannot be changed without migrating the account.
   - **Local bind**: leave `127.0.0.1` unless you want to expose the core on the LAN.
   - **Relay**: relay URL (`https://relay.example.org`) and the token obtained from registration. You can leave it empty for a P2P-only mode.
3. Press **Start**. The app waits for the core to actually be listening before showing the timeline. If the core does not start, the error is shown in the setup screen.

The configuration is saved atomically in the app data directory, with `600` permissions on POSIX. If the file is corrupted, the app tries to recover it from the backup and, if it fails, warns you instead of silently overwriting.

## Timeline

- Federated, local and "following" timelines are separate.
- Quick filters let you see only media, only replies or only boosts.
- Actions (like, boost, reply, delete) are available from the note menu.
- Media open in the integrated viewer: swipe to navigate, tap to toggle audio.

## Publishing

- The composer accepts text, media and visibility (public, unlisted, followers, direct).
- The text is always converted to HTML with escaping: the characters `<`, `>`, `&`, `"` and `'` do not break rendering nor open injection.
- MFM syntax (`**bold**`, `:emoji:`) is supported.
- Direct notes do not appear in the public outbox.

## Chat

- Chats are 1:1 or group, with P2P transport when possible.
- Messages are encrypted with AES-256-GCM and per-message keys wrapped with RSA-OAEP. The relay does not see the content.
- It supports reactions, files, GIFs and read status.
- Known limit: there is no *forward secrecy*. If a private key is compromised, past messages become readable.

## Notifications

- Notifications are local; no external push service is needed.
- On Android the `POST_NOTIFICATIONS` permission is needed (requested on first use) and the `INTERNET` permission.
- The notification list updates at the top without clearing those already read.

## Settings

- **Security**: relay token, key rotation, profile visibility.
- **Privacy**: telemetry consent (optional), media cache, per-domain blocking.
- **Moderation**: lists of blocked actors and domains.
- **Backup**: export and import configuration and preferences in JSON. The backup does **not** contain the private key: that lives in the core data directory and must be saved separately if you want to be able to migrate.
- **Language**: Italian and English.

Every change to the configuration restarts the core and waits for it to be ready. If the new value does not work, the app restores the previous configuration.

## Common problems

| Symptom | Probable cause | Remedy |
|---|---|---|
| "Core not started" | port already in use or invalid username | change the bind or fix the username |
| Empty timeline | relay unreachable or wrong token | verify the token in settings |
| No incoming federation | public domain does not resolve to the relay | check DNS and the tunnel |
| Endless spinner | the core is not responding | the app detects the failure within a few seconds and attempts a restart |
| Media not visible | relay media backend not configured | check `FEDI3_RELAY_MEDIA_BACKEND` |

For the relay see [deploy_relay.md](deploy_relay.md) and [troubleshooting.md](troubleshooting.md). To build the app from source see [build_linux.md](build_linux.md).

### Credentials and Linux keyring

Linux builds require `libsecret` and an active Secret Service in the user
session, for example GNOME Keyring or KWallet with the Secret Service
enabled. To build, install `libsecret-1-dev` and `libjsoncpp-dev` on
Debian/Ubuntu, or `libsecret` and `jsoncpp` on Arch.

If the keyring is locked or unavailable, Fedi3 shows a recovery screen:
unlock the keyring and press **Retry**. The program keeps configuration and
account and does not propose creating a new profile in this condition. A
corrupted JSON is reported separately. Do not delete `config.json` to solve a
keyring problem.
