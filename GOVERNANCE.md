# Fedi3 Governance

## Stewardship

Fedi3 is stewarded by [RedHunt07](https://github.com/redhunt07) as the reference
implementation. The project uses a maintainer-led model: the maintainer sets the
direction, reviews contributions and cuts releases, while contributors shape the
roadmap through issues, pull requests and protocol discussions.

Stewardship of the codebase does not imply ownership of the network. Anybody may
run a relay, build a client or fork the project under the terms of the license.

## Decision model

Technical decisions are guided by, in order:

1. Security.
2. Privacy.
3. Decentralization and user sovereignty.
4. Interoperability with the wider fediverse.
5. Maintainability and operational simplicity.

Popularity alone is not a valid argument. Decisions are made in public where
possible: design discussions and rationale live in issues or pull requests, and
the resulting behavior is documented in `docs/`. When consensus is not reached,
the maintainer decides and records the reasoning. Reversible choices are
preferred over irreversible ones, and changes that affect the network require
more evidence than local changes.

## Protocol evolution

Protocol changes must be documented, versioned and tested. Backward
compatibility is the default; breaking changes require a clear justification, a
migration path and an announcement. P2P extensions must remain optional and
negotiable so that classic ActivityPub instances keep working. See
[PROTOCOL_COMPATIBILITY.md](PROTOCOL_COMPATIBILITY.md).

## Releases

Fedi3 follows [Semantic Versioning](https://semver.org/). The project is
pre-1.0, so minor releases may contain breaking changes, but they are announced
and documented. The supported line is 0.11.x; older lines receive fixes only on
a best-effort basis. Release tags are created from `main`, must match the
version in `app/pubspec.yaml` and are published with checksums. Release notes
follow `CHANGELOG.md` and `ANNOUNCEMENTS.md`.

## Security

Critical vulnerabilities take priority over feature work. Fixes and advisories
are published together, and the advisory is public once a fixed release is
available. Details are in [SECURITY.md](SECURITY.md).

## Contributions and review

Pull requests are reviewed against the criteria in [CONTRIBUTING.md](CONTRIBUTING.md):
tests, documentation (English and Italian), protocol compatibility and scope.
CI must pass before merge. Review is best effort and conducted in public, except
for security reports.

## Forks and naming

Forks are welcome and are governed by [FORK_POLICY.md](FORK_POLICY.md). A fork
must use a distinct name unless it is presented as an unmodified copy; see
[TRADEMARK_AND_NAME.md](TRADEMARK_AND_NAME.md). The network is defined by
compatibility, not by a single codebase; see
[NETWORK_INTEGRITY.md](NETWORK_INTEGRITY.md).

## Contact

- General questions, bug reports and proposals: GitHub issues in this repository.
- Security reports: private GitHub Security Advisories.
- Maintainer: [@redhunt07](https://github.com/redhunt07).

## Changes to this document

Governance changes are proposed as a pull request or an issue with the rationale
and the practical effect. The maintainer decides, and the decision is recorded
in the pull request that updates this file.
