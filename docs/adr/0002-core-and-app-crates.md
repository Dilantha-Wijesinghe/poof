# 2. Split into a UI-agnostic core and a thin app

- Status: accepted
- Date: 2026-10-16

## Context

Disk logic (scanning, rules, deletion, history) is the risky, valuable part
and must be heavily tested. UI code changes often.

## Decision

Use two crates: `poof-core` holds all domain logic and macOS integration with
no GUI dependency, and `poof-app` holds only presentation and orchestration.
Dependencies point from app to core only.

## Consequences

- Core is unit-tested against temporary directories without a window.
- The same core powers the GUI, the `--check` background mode and the
  `report` example. A CLI or menu-bar app could reuse it unchanged.
- Data passed to the UI must be `Clone + Send`. Large results such as the
  scan tree travel as `Arc`s.
