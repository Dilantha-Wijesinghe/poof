# Security Policy

Poof deletes files, so we treat safety bugs as security bugs.

## Supported versions

Only the latest release receives fixes.

## Reporting a vulnerability

Please **do not open a public issue**. Use GitHub's
[private vulnerability reporting](https://github.com/Dilantha-Wijesinghe/poof/security/advisories/new)
instead. Include steps to reproduce and the Poof and macOS versions. You
should hear back within a few days.

Examples of in-scope issues:

- Poof deleting, or offering to delete, anything outside what a rule or the
  user explicitly targeted
- A way around the protected-path guard (`crates/poof-core/src/remove.rs`)
- Leftover detection flagging data of an installed app
- Command or AppleScript injection through file names
- The LaunchAgent being abusable by other users or processes
- A way to make the updater install anything that wasn't signed with Poof's
  update key

## Safety model

See [docs/adr/0004-safety-model.md](docs/adr/0004-safety-model.md). In short:

- every removal goes through a single guarded function
- Explore only moves items to the Trash
- only regenerable data is preselected
- every action is logged to `~/Library/Logs/Poof/operations.log`
- Poof collects no data; its only network request is the daily update check
  to GitHub
- updates are installed only if their minisign signature matches the public
  key built into the app, and the bundle identifier, version and code
  signature check out (see docs/adr/0006-self-updates.md)
