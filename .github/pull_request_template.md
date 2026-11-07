## Summary

<!-- What does this pull request change, and why? Link a related issue. -->

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

## Validation

<!-- Commands you ran and their results. State anything you could not validate. -->

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo test --workspace --locked`
- [ ] `cd app && flutter analyze --fatal-infos`
- [ ] `cd app && dart format --output=none --set-exit-if-changed lib test`
- [ ] `cd app && flutter test`
- [ ] `bash scripts/verify_platform_config.sh`
- [ ] Documentation links and examples checked

## Checklist

- [ ] I followed the repository's contribution guidance.
- [ ] I preserved licenses, copyright notices and upstream attribution.
- [ ] I included no credentials, private data or generated build output.
- [ ] I updated the relevant documentation (English default, Italian counterpart in `docs/it/`).
- [ ] I added or updated tests for behavior changes.
- [ ] I did not break protocol compatibility with classic ActivityPub instances.

## Notes for reviewers

<!-- Edge cases, limitations, follow-ups. -->
