## Summary

<!-- What does this pull request change, and why? Link the related issue. -->

## Type of change

- [ ] Bug fix
- [ ] Security hardening
- [ ] Protocol / federation behavior
- [ ] Relay or infrastructure
- [ ] Client (UI/UX)
- [ ] Documentation
- [ ] Tests only
- [ ] Refactor without behavior change
- [ ] Other (describe below)

## Scope

<!-- One concern per pull request. List the components touched and anything intentionally left out. -->

- Components affected:
- Related issue:
- Follow-ups or known limitations:

## Validation

<!-- Commands you ran and their results. State anything you could not validate. -->

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo test --workspace --locked`
- [ ] `cd app && flutter analyze --fatal-infos`
- [ ] `cd app && dart format --output=none --set-exit-if-changed lib test`
- [ ] `cd app && flutter test`
- [ ] `bash scripts/verify_platform_config.sh`
- [ ] `python3 scripts/tests/test_deploy_config.py`
- [ ] `python3 scripts/tests/test_release_contract.py`
- [ ] `python3 scripts/tests/test_relay_directory.py`
- [ ] `python3 scripts/tests/test_backup_scripts.py`
- [ ] ShellCheck on the changed scripts
- [ ] Manual end-to-end check (describe it below)

## Documentation

- [ ] I updated the English documentation in `docs/` where behavior changed.
- [ ] I updated the Italian counterpart in `docs/it/` with the same content.
- [ ] I updated `CHANGELOG.md` for a user-visible change.
- [ ] I added a dated summary to `ANNOUNCEMENTS.md` and `ANNOUNCEMENTS.it.md` for a user-visible change.
- [ ] No documentation change is needed (explain why).

## UI changes

<!-- Required for client changes; delete this section when not applicable. -->

- [ ] Screenshots or a short recording are attached.
- [ ] Checked on Linux desktop.
- [ ] Checked on Android (when mobile layout or permissions are affected).
- [ ] Follows the existing Aurora design system and keeps screens visually consistent.
- [ ] Keyboard navigation, focus order and text scaling were considered.

## Compatibility and security

- [ ] I preserved protocol compatibility with classic ActivityPub instances, or documented the deviation.
- [ ] I added or updated tests for the new behavior or the regression.
- [ ] I verified that no credentials, tokens, private keys or personal data are included.
- [ ] I preserved SPDX headers, copyright notices and attribution.
- [ ] I did not commit generated output (`dist/`, `target/`, `build/`, native libraries).
- [ ] I did not change the FFI ABI unintentionally, and `panic = "unwind"` still holds.

## Notes for reviewers

<!-- Edge cases, trade-offs, limitations, follow-ups. -->
