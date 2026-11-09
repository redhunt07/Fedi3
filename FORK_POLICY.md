# Fedi3 Fork Policy

Forks are welcome. Fedi3 is AGPL-3.0-or-later software, and the license already
guarantees the freedom to study, modify, run and redistribute it. This policy
explains what the project expects from forks in return.

## You may

- Change the UI, theme and navigation.
- Add or remove features.
- Improve performance, resource usage and battery life.
- Customise behavior, defaults and configuration.
- Target different platforms or packaging.
- Rebrand the client, subject to the naming rules below.

## Requirements

- **Keep the license.** Forks must remain AGPL-3.0-or-later. If you run a
  modified version as a network service, users interacting with it over the
  network must be able to obtain the corresponding source.
- **Preserve attribution.** Keep copyright notices, SPDX headers and the
  [NOTICE](NOTICE) file, and state clearly what you changed.
- **Preserve protocol compatibility** whenever possible. A fork that federates
  must keep ActivityPub behavior, HTTP signature verification and the shared
  relay contract intact, or document every deviation.
- **Use a distinct name and identity.** Do not present a fork as the official
  Fedi3 client or relay. "Based on Fedi3" is fine; see
  [TRADEMARK_AND_NAME.md](TRADEMARK_AND_NAME.md).
- **Do not introduce tracking, profiling, advertising or data resale.** Any
  telemetry must be opt-in and documented.
- **Do not lock users into a closed ecosystem.** Data export, open formats and
  the ability to switch relays or instances must remain available.
- **Keep security fixes flowing.** Track upstream security advisories and merge
  or backport fixes, or document why a fix does not apply.

## Recommended

- Upstream changes that are generally useful instead of maintaining a permanent
  divergence.
- Keep the diff small and documented, for example in a `FORK.md` or a section of
  the README listing the differences.
- Keep the relay directory entry and the release checksums accurate if you
  publish builds.
- Run the same test suite as upstream before releasing.

## Not required

- Keeping the same UI, dependencies, supported platforms or release cadence.
- Asking for permission before forking.

## Compatibility is a responsibility

A fork inherits the network, not just the code. Intentional fragmentation
without a technical reason is discouraged; see
[NETWORK_INTEGRITY.md](NETWORK_INTEGRITY.md).
