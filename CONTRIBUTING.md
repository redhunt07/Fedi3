# Contributing to FEDI3

Thank you for contributing to FEDI3. This project exists to protect
decentralization, privacy and protocol unity.

## Before opening an issue

1. Check the [documentation](docs/README.md) and the [FAQ](docs/faq.md).
2. Search existing issues and pull requests.
3. Reduce the problem to the smallest reproducible input, configuration or
   sequence.
4. Remove tokens, passwords, private keys and personal data from logs and
   screenshots.

A useful bug report includes the affected component (relay, core, app), the
commit or version, the operating system, the expected behavior and the observed
behavior.

## Development setup

Prerequisites: Rust stable, Flutter 3.38 or newer, and the Linux desktop
dependencies listed in the [Linux build guide](docs/build_linux.md).

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
python3 scripts/tests/test_backup_scripts.py
python3 scripts/tests/test_deploy_config.py
```

Run the full suite before opening a pull request. A change that affects
behavior must come with a test; a change that affects deployment must come with
a matching check in `scripts/tests/`.

## Code rules

- Preserve the original copyright headers and add your author line when you
  modify a file.
- Do not remove existing attribution.
- Keep documentation in sync with behavior.
- Prefer small, reviewable pull requests over large mixed changes.
- Do not introduce tracking, profiling, advertising or data resale.
- Keep the FFI boundary panic-safe (`panic = "unwind"` is required by
  `catch_unwind`).

## Network responsibility

Any change affecting protocol, federation or P2P behavior must be documented
and tested. Intentional fragmentation of the fediverse is discouraged: FEDI3
must remain interoperable with classic ActivityPub instances whenever possible.

## UI freedom

UI and graphical customization is fully encouraged. UI forks must not silently
alter protocol behavior.

## Forks

Forks are encouraged for performance improvements, security enhancements,
UI/UX customization, accessibility and experimental features. Forks must
preserve protocol compatibility whenever possible and keep the AGPL-3.0-or-later
license.

## Commit and pull request expectations

- Write commit messages in the imperative mood and keep them focused.
- Describe the concrete problem, the resulting behavior and the validation you
  ran in the pull request.
- Link a related issue when applicable.
- State anything you could not validate.

## License

All contributions are released under AGPL-3.0-or-later. See [LICENSE](LICENSE).
