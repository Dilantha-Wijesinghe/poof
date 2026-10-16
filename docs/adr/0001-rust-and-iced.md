# 1. Rust with iced for a native GUI

- Status: accepted
- Date: 2026-10-16

## Context

Poof must scan millions of files quickly, stay light in memory, and present a
polished macOS interface. The main Rust GUI options were egui (immediate
mode), iced (retained, Elm architecture, wgpu), Slint (DSL, GPL or commercial
licensing for some uses), GPUI (young, sparse documentation) and Tauri (web
UI in a webview).

## Decision

Use **iced 0.14**.

- Its Elm architecture (State → view → Message → update) gives a predictable,
  testable structure that scales from one screen to many.
- Its canvas API is good enough for an interactive treemap and charts.
- It supports custom themes and follows the system light/dark appearance.
- It has a transparent title bar and full-size content view on macOS.
- It is MIT licensed, matching ours, with an active community.

## Consequences

- iced has no native AppKit widgets, so we own the visual design through
  `style.rs` tokens.
- Typography is bundled (Inter) so text renders identically everywhere.
- Upgrading iced across minor versions can require API changes. All iced usage
  is confined to `poof-app`.
