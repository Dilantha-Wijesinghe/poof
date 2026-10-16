# 6. Built-in signed updates and a stable signing identity

- Status: accepted
- Date: 2026-10-16

## Context

Users should get new versions without reinstalling by hand. Poof isn't
notarized, so three practical problems come up:

- Downloading a new DMG means facing Gatekeeper again.
- Every ad-hoc-signed build has a new code identity (its designated
  requirement is a `cdhash`), so macOS forgets Full Disk Access after each
  update.
- Homebrew dropped support for casks that fail Gatekeeper checks.

We considered Sparkle and a small updater of our own.

## Decision

**Updater.** We wrote our own updater, `poof_core::update`. It covers the
whole flow:

1. Once a day, and on demand from Settings, it asks the GitHub Releases API
   for the latest release using the system `curl`. It never offers drafts or
   pre-releases.
2. If a newer version exists, the sidebar shows a notice. Installing takes one
   click and a confirmation.
3. It downloads `Poof-X.Y.Z.zip` and `Poof-X.Y.Z.zip.minisig`, and verifies the
   minisign signature against the public key compiled into the app
   (`crates/poof-core/update-key.pub`).
4. It unpacks the archive with `ditto`, then checks the bundle identifier, the
   version and the code signature.
5. It copies the new app next to the old one, swaps them with two renames
   (rolling back on failure), and relaunches.

Files downloaded by the app itself aren't quarantined, so Gatekeeper doesn't
interrupt updates.

**Signing.** Releases are signed with Poof's own self-signed certificate,
created by `scripts/create-signing-identity.sh` and kept in GitHub secrets.
We sign with `rcodesign`, which reads the `.p12` directly. Apple's `codesign`
won't use a certificate the system doesn't trust, and adding trust would mean
changing machine-wide trust settings. The resulting designated requirement
pins the certificate, so it is the same for every release and macOS keeps
privacy permissions across updates.

## Why not Sparkle

Sparkle is excellent, but here it would mean:

- embedding an Objective-C framework
- writing unsafe glue code to drive it from a winit/iced app
- generating and hosting an appcast
- working around library-validation issues with non-Developer-ID signatures

The core of our updater is a few hundred lines of Rust with integration tests
that cover:

- a real download (`file://`)
- signature checks
- the bundle swap
- rejection of wrong keys and wrong versions

## Consequences

- **Two secrets need backing up**, `update-signing.key` and `codesign.p12`
  (both in `~/.poof-release`):
  - If the update key is lost, existing installs can't verify new releases.
    Users would have to reinstall manually once, from a build with a new key.
  - If the certificate is lost, users re-grant Full Disk Access once.
- **The release workflow fails if either secret is missing**, so an unsigned
  update is never published.
- **One network request:** the daily update check to `api.github.com`. It sends
  no user data, and Settings can turn it off.
- **First installs still need "Open Anyway".** Removing that, and offering a
  Homebrew cask, requires an Apple Developer ID and notarization. The signing
  step is the only thing that would change.
