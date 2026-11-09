# Contributing to Fedi3

Fedi3 is a federated social platform built around a Flutter client, a local Rust
core, a public relay and an optional P2P layer. Contributions of code, tests,
documentation, translations, relay listings and reproducible bug reports are
welcome.

The project is pre-1.0 and moves quickly. Read this file, the [documentation](docs/README.md)
and the [FAQ](docs/faq.md) before starting, and keep changes small and
reviewable. By contributing you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md)
and to license your work under AGPL-3.0-or-later.

## Before opening an issue

1. Search existing issues and pull requests.
2. Check the [documentation](docs/README.md), the [FAQ](docs/faq.md) and the
   [troubleshooting guide](docs/troubleshooting.md).
3. Reduce the problem to the smallest reproducible input, configuration or
   sequence.
4. Remove tokens, passwords, private keys and personal data from logs and
   screenshots.

A useful bug report names the affected component (relay, core, app, protocol or
deployment), the version or commit, the operating system, the expected behavior
and the observed behavior. Use the issue templates in `.github/ISSUE_TEMPLATE/`.

Security problems do not belong in a public issue: report them privately as
described in [SECURITY.md](SECURITY.md).

## Development setup

Prerequisites:

- Rust stable, with `rustfmt` and `clippy`.
- Flutter 3.38 or newer for the client.
- The Linux desktop dependencies listed in the [Linux build guide](docs/build_linux.md)
  (`libsecret`, `jsoncpp`, `libmpv`, GTK, WebKitGTK and the audio libraries).
- Docker and Docker Compose for the relay stack.
- `shellcheck` and Python 3 for the script and deployment checks.

```bash
git clone https://github.com/redhunt07/Fedi3.git
cd Fedi3
cargo fetch --locked
cd app
flutter pub get
```

The local core is built with `scripts/build_core.sh` (Linux/macOS) or
`scripts/build_core.ps1 -Profile release` (Windows). Both scripts compile
`crates/fedi3_core` and copy the native library where the Flutter app expects
it; do not copy artifacts by hand. The relay runs from the repository root with
`docker compose` after copying `.env.example` to `.env`.

## Repository layout

| Path | Content |
| :--- | :--- |
| `app/` | Flutter client (Linux, Android, iOS, macOS, Windows) |
| `crates/fedi3_core/` | Local core: ActivityPub, chat, P2P, storage, FFI |
| `crates/fedi3_relay/` | Public relay: federation compatibility, media, spool |
| `crates/fedi3_protocol/` | Shared protocol types and crypto helpers |
| `crates/fedi3_p2p_infra/` | Mailbox and circuit-relay infrastructure |
| `deploy/`, `docker-compose.yml` | Deployment assets and service configuration |
| `docs/` | English documentation (Italian in `docs/it/`) |
| `scripts/` | Build, install, backup, diagnostic and verification scripts |

Generated output (`dist/`, `target/`, `build/`, `AppDir/`, `*.so`, `*.dll`) is
not committed.

## Branches and commits

- Branch from the current `main`. Use a descriptive prefix:
  `feat/<topic>`, `fix/<topic>`, `docs/<topic>`, `test/<topic>`,
  `refactor/<topic>`, `ci/<topic>` or `relay/<hostname>` for relay directory
  entries.
- Do not push directly to `main`; every change goes through a pull request.
- Follow [Conventional Commits](https://www.conventionalcommits.org/): a typed
  prefix such as `feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `perf:`,
  `build:`, `ci:` or `chore:`, an optional scope (`fix(relay):`,
  `docs(app):`), and an imperative subject of at most 72 characters.
- Explain the motivation in the commit body when the subject is not enough, and
  reference the related issue.
- Keep each commit coherent and buildable. Rebase on `main` instead of merging
  it back into your branch.

Examples:

```text
feat(core): negotiate relay capabilities on tunnel setup
fix(app): keep the chat composer above the Android navigation bar
docs(relay): document the PostgreSQL quota settings
```

## Tests before a pull request

Run the checks that cover your change, and run the full suite before requesting
review. CI runs the same commands on every pull request.

```bash
# Rust workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

# Flutter client
cd app
flutter pub get
flutter analyze --fatal-infos
dart format --output=none --set-exit-if-changed lib test
flutter test

# Platform configuration and deployment contracts
bash scripts/verify_platform_config.sh
python3 scripts/tests/test_deploy_config.py
python3 scripts/tests/test_release_contract.py
python3 scripts/tests/test_relay_directory.py
python3 scripts/tests/test_backup_scripts.py
python3 scripts/tests/test_appimage_host_libs.py
```

Shell scripts must pass ShellCheck with the same file list used by CI:

```bash
shellcheck -x -P scripts scripts/lib/dotenv.sh scripts/backup_relay_pg.sh \
  scripts/restore_relay_pg.sh scripts/backup_relay_media.sh \
  scripts/restore_relay_media.sh scripts/relay_diagnose.sh \
  scripts/install_server.sh scripts/install_yunohost.sh
```

The PostgreSQL media-quota test is opt-in and needs a database:

```bash
FEDI3_TEST_POSTGRES_URL=postgres://user:pass@127.0.0.1:5432/db \
  cargo test -p fedi3_relay postgres_media_quota_reservation_is_atomic --locked -- --ignored
```

Requirements for the change itself:

- New behavior needs tests; a bug fix needs a regression test that fails before
  the fix.
- A deployment or configuration change needs a matching check in
  `scripts/tests/`.
- Protocol, federation and P2P changes need tests that exercise both Fedi3
  peers and classic ActivityPub peers when applicable.
- State explicitly in the pull request anything you could not validate.

## Scope of changes

- Keep one concern per pull request. Avoid mixing refactors, formatting churn
  and behavior changes.
- Do not modify generated files or vendored dependencies.
- Preserve SPDX headers, copyright notices and upstream attribution; add your
  own copyright line when you substantially modify a file.
- Do not introduce tracking, profiling, advertising or data resale.
- Keep the FFI boundary panic-safe: `panic = "unwind"` is required because the
  FFI layer uses `catch_unwind`.
- Keep the native library ABI stable unless the change is intentional and
  documented.

## Documentation and localisation

Documentation is bilingual by policy:

- English is the default language and lives in `docs/`.
- Every user-facing document has an Italian counterpart under `docs/it/` with
  the same structure and content.
- Update both versions in the same pull request, and keep links, commands and
  configuration keys identical.
- Root policy files (`CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`,
  `GOVERNANCE.md`, `FORK_POLICY.md`, `ETHICS.md`, `NETWORK_INTEGRITY.md`,
  `PROTOCOL_COMPATIBILITY.md`, `TRADEMARK_AND_NAME.md`) are English only. The
  project README has an Italian counterpart in `README.it.md`.
- User-visible changes also need a `CHANGELOG.md` entry and a dated summary in
  `ANNOUNCEMENTS.md` and `ANNOUNCEMENTS.it.md`, following the existing format.

## UI changes

UI and graphical customisation are encouraged, but UI forks must not silently
change protocol behavior. For client changes:

- Include screenshots or a short recording in the pull request.
- Check the affected screens on Linux desktop and, when the change touches
  mobile layout, on an Android build.
- Follow the existing Aurora design system and keep the interface visually
  consistent across screens.
- Keep accessibility in mind: focus order, contrast, touch targets and text
  scaling.

## Network responsibility

Any change affecting protocol, federation or P2P behavior must be documented and
tested. Intentional fragmentation of the fediverse is discouraged: Fedi3 must
stay interoperable with classic ActivityPub instances whenever possible. See
[NETWORK_INTEGRITY.md](NETWORK_INTEGRITY.md) and
[PROTOCOL_COMPATIBILITY.md](PROTOCOL_COMPATIBILITY.md).

## Adding your relay

Public relays are discovered through the shared directory in
[`relays/relays.json`](relays/relays.json). To add yours:

1. Verify HTTPS with a valid certificate and `GET /healthz` returning `200 ok`.
2. Add one entry to `relays/relays.json`, keep the array ordered by `added`
   date, and update `updated_at`.
3. Run `python3 scripts/tests/test_relay_directory.py`.
4. Copy the updated file to `app/assets/relays.json`; CI checks that the two
   copies match.
5. Open a pull request titled `relay: add <hostname>` and include the relay URL,
   the operator and a contact where users can request a token.

Relay tokens are not stored in the directory: each relay issues tokens to its
own users. See [`relays/README.md`](relays/README.md) and
[`docs/relay_directory.md`](docs/relay_directory.md) for the full rules.

## Secrets and private data

- Never commit `.env`, tokens, passwords, private keys, runtime databases or
  build output.
- The relay keeps only SHA-256 hashes of user tokens; admin tokens must be at
  least 32 characters.
- The core stores tokens in the OS keyring and private keys in its data
  directory with restrictive permissions.
- Redact bearer tokens, `X-Fedi3-Internal` values and personal data from logs
  and screenshots before posting them.
- Do not log message bodies, media URLs with signed query strings, or
  credentials at info level.

## Review expectations

- A maintainer reviews every pull request; CI must be green before merge.
- Expect questions about tests, compatibility and scope. Reviews are about the
  change, not the contributor.
- Keep the discussion in the pull request and respond to review comments before
  asking for another pass.
- The maintainer may squash commits, adjust the title or ask to split a large
  change.
- Review is best effort: this is a small project, so a polite reminder after a
  week is appropriate.

## Forks

Forks are encouraged for performance, security, UI/UX, accessibility and
experimental features. Forks must preserve protocol compatibility whenever
possible, keep the AGPL-3.0-or-later license and use a distinct name. See
[FORK_POLICY.md](FORK_POLICY.md) and [TRADEMARK_AND_NAME.md](TRADEMARK_AND_NAME.md).

## License

All contributions are released under AGPL-3.0-or-later. See [LICENSE](LICENSE)
and [NOTICE](NOTICE). Network-exposed modifications must remain open source.
