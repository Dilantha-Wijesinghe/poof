<p align="center">
  <img src="docs/images/icon.png" width="128" alt="Poof icon">
</p>

<h1 align="center">Poof</h1>

<p align="center">
  <b>Find out what's eating your Mac's storage, and get it back.</b><br>
  A fast, native, open-source disk analyzer and cleaner for macOS, written in Rust.
</p>

<p align="center">
  <a href="https://github.com/Dilantha-Wijesinghe/poof/actions/workflows/ci.yml"><img src="https://github.com/Dilantha-Wijesinghe/poof/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/Dilantha-Wijesinghe/poof/releases/latest"><img src="https://img.shields.io/github/v/release/Dilantha-Wijesinghe/poof" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/platform-macOS%2012%2B-lightgrey" alt="macOS 12+">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT license"></a>
</p>

---

You had 150 GB free last week. Today you have 35 GB, and you have no idea why.
Poof answers that question in seconds, then helps you take the space back,
safely.

## What it does

- **Overview.** Shows how much space is used, purgeable and free, explained in
  plain words. Lists the *usual suspects* behind a mysterious "System Data"
  number, measured directly without a full scan: Docker's disk image,
  simulators, swap, iCloud downloads, Messages attachments, device backups,
  macOS installers and more.
- **Explore.** A parallel scanner walks your home folder or the whole disk
  (about 2 million files in roughly 15 seconds on an M-series Mac). You can
  drill in through an interactive treemap and a sorted list. Items that grew
  since your last scan are flagged inline.
- **Changes.** Every scan is saved as a compact summary, so Poof can tell you
  *exactly which folders grew* between two dates. Growth is attributed to the
  deepest folder that explains it, so totals are never double-counted. A
  free-space timeline shows when the drop happened.
- **Clean Up.** A reviewed catalog of reclaimable data: app, browser and web-app
  caches, logs, Xcode DerivedData and device support, package-manager caches
  (npm, pnpm, Yarn, Bun, Cargo, pip, uv, Gradle, CocoaPods, Homebrew), build
  folders in projects you haven't touched lately, leftovers from apps you
  removed, old installers, iOS backups and the Trash.
- **Background monitoring** (optional). A tiny LaunchAgent checks free space
  every 30 minutes and sends you a notification if it drops suddenly or runs
  low. You find out the day it happens, not weeks later.
- **Local snapshots.** See Time Machine's local snapshots and thin them with one
  click. Snapshots are a common reason deleting files seems to free nothing.

## Safety first

Most "Mac cleaner" apps have earned their bad reputation. Poof is built to be
the opposite:

- **Nothing is removed without your explicit review and confirmation.** Only
  items marked **Safe** (data that rebuilds itself automatically) are
  preselected. **Review** items never are.
- **Explore always uses the Trash**, so anything you remove there can be put
  back.
- **A hard-coded guard refuses protected locations.** That includes `/System`,
  `/usr`, your home folder itself, `~/Library`'s structure, `~/Documents`,
  `~/Desktop`, Keychains and more, no matter what any rule says.
- **App leftovers are matched by bundle identifier, not guessed from folder
  names.** Anything sharing a vendor or product name with an installed app is
  never flagged.
- **Every removal is logged** to `~/Library/Logs/Poof/operations.log` with its
  size and path.
- **Poof never reads file contents**, never phones home, and has no telemetry,
  accounts or ads.
- **It's fully open source** (MIT), so you can audit exactly what it does. See
  [SECURITY.md](SECURITY.md).

## Install

1. Download the latest `Poof-x.y.z.dmg` from
   [Releases](https://github.com/Dilantha-Wijesinghe/poof/releases/latest). It
   is a universal build for Apple silicon and Intel.
2. Open the disk image and drag **Poof** to **Applications**.
3. Poof is not notarized by Apple yet, so Gatekeeper will block the first
   launch. Either right-click Poof → **Open** → **Open**, or run:

   ```sh
   xattr -dr com.apple.quarantine /Applications/Poof.app
   ```

4. For complete results, grant **Full Disk Access**: System Settings → Privacy
   & Security → Full Disk Access → add Poof. Poof links you straight there.
   Without it, Mail, Messages, Safari, the Trash and other apps' data can't be
   measured.

Verify a download with the `SHA256SUMS.txt` attached to each release.

## Build from source

Requires macOS 12+ and Rust 1.88+ (`rustup` recommended).

```sh
git clone https://github.com/Dilantha-Wijesinghe/poof.git
cd poof
cargo run --release -p poof             # run the app
scripts/bundle.sh                       # build dist/Poof.app, .dmg and .zip (universal)
cargo run --release -p poof-core --example report   # quick storage report in the terminal
```

## Where does "System Data" come from?

When macOS shows a huge **System Data** bar, it's usually one of these. Poof
measures each of them on the Overview screen.

| Culprit | Why it grows | What to do |
|---|---|---|
| Docker / VM disk images | Grow as you pull images; never shrink on their own | `docker system prune -a`, then lower the disk limit |
| Local Time Machine snapshots | Keep deleted files on disk for ~24 h or longer | Thin them from Overview |
| Simulators & runtimes | Each Xcode update adds runtimes | Xcode → Settings → Components; `xcrun simctl delete unavailable` |
| Caches | Browsers, Electron apps and package managers cache aggressively | Clean Up |
| Swap & sleep image | Memory pressure | Restart; quit memory-hungry apps |
| iOS backups, Messages, iCloud downloads | Silent, gradual growth | Overview tells you where to manage them |

"Purgeable" space counts as available in Finder, but it is only released when
macOS decides it needs it. Poof shows both numbers so the gap is no longer a
mystery.

## Project layout

```
crates/poof-core   UI-agnostic engine: scanning, cleanup rules, history, monitor, macOS APIs
crates/poof-app    The iced GUI (Elm architecture), binary name `poof`
xtask              Dev tooling (`cargo xtask icon`)
packaging/macos    Info.plist and app icon sources
scripts/bundle.sh  Builds the signed .app, .dmg and .zip
docs/              Architecture notes and decision records
```

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the design, and
[CONTRIBUTING.md](CONTRIBUTING.md) to get involved. Adding a cleanup target is
usually a single data entry in
[`catalog.rs`](crates/poof-core/src/cleanup/catalog.rs).

## Roadmap

- App uninstaller (remove an app together with its data)
- Duplicate-file finder (opt-in, never touching Photos libraries)
- Menu-bar extra with a live free-space readout
- Developer ID signing and notarization, plus a Homebrew cask

## Acknowledgements

Poof stands on the shoulders of [iced](https://iced.rs) and
[rayon](https://github.com/rayon-rs/rayon), and uses the
[Inter](https://rsms.me/inter/) typeface (SIL OFL) and
[Lucide](https://lucide.dev) icons (ISC). It was inspired by GrandPerspective,
DaisyDisk, dua-cli and Mole; no code was copied from any of them.

## License

[MIT](LICENSE) © Dilantha Wijesinghe and the Poof contributors.
