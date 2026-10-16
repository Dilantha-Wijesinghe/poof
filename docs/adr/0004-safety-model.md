# 4. Safety model for deletion

- Status: accepted
- Date: 2026-10-16

## Context

Cleaner apps are widely distrusted. Many have deleted user data such as
photos "detected" as duplicates, or broken systems. Poof must earn trust.

## Decision

- **One code path** (`remove::remove`) performs every deletion. It first
  applies `remove::guard`, which refuses system trees, structural folders
  (home, `~/Library` and its direct children, standard home folders,
  Keychains) and any relative or `..` path.
- **Explore always moves items to the Trash** (via `NSFileManager`, with no
  Finder scripting).
- **Clean Up preselects only `Safety::Safe` rules**, meaning data the system or
  app regenerates without user impact. `Safety::Review` items are never
  preselected, and confirmation dialogs call them out.
- **App leftovers** are only considered for bundle-identifier-named locations,
  and only when no installed app shares the vendor or product name.
- **Every removal is appended to an operations log.**

## Consequences

- Some reclaimable space is deliberately not offered (for example
  ambiguously named Application Support folders).
- The guard is unit-tested with both blocked and allowed paths.
