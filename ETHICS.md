# Fedi3 Ethics

These principles describe what Fedi3 is for and what it refuses to become. They
guide design decisions, reviews and protocol changes.

## Principles

- **Privacy first.** Collect the minimum needed to work. Private keys, tokens and
  message contents stay on the user's device whenever possible.
- **No tracking, no profiling.** No analytics SDKs, no advertising identifiers,
  no behavioral profiles, no data resale.
- **No advertising.** The timeline is not for sale. Sponsored content is opt-in
  at most.
- **Local moderation.** Each user and each relay decides what they host and see.
  Moderation is transparent and documented, not silently centralized.
- **User sovereignty.** Users own their identity, keys, data and follow graph.
  They can export their data, move to another relay and self-host.
- **Consent-based telemetry.** Client diagnostics stay on the device and are
  shared only when the user chooses to export them. Relay statistics are
  server-side operational data, not user tracking.
- **Interoperability over lock-in.** The fediverse is larger than Fedi3. Speak
  ActivityPub with classic instances instead of inventing a private island.
- **Accessibility and respect.** The client should be usable with a keyboard, a
  screen reader and different text sizes, and the community follows the
  [Code of Conduct](CODE_OF_CONDUCT.md).
- **Transparency.** Document limitations honestly, including the absence of full
  forward secrecy in chat v1 and the residual risks of the relay and spool.

## What this means in practice

- A feature that requires collecting personal data must justify it and offer an
  off switch where possible.
- A dependency that phones home by default must be configured not to, or
  replaced.
- A protocol extension must be optional and documented, never a requirement for
  basic federation.
- A performance optimization must not weaken signature verification, SSRF
  protection or encryption.

Technology serves people, not the other way around.
