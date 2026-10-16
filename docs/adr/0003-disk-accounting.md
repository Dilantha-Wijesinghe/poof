# 3. Count allocated blocks, dedupe hard links, never cross devices

- Status: accepted
- Date: 2026-10-16

## Context

Users compare Poof's numbers against Finder and System Settings. Logical file
sizes overstate sparse files (Docker, VMs) and understate nothing.
Double-counting through APFS firmlinks or hard links makes totals impossible
to trust.

## Decision

- Size = `st_blocks × 512` from `lstat`. Never follow symlinks.
- Count each `(device, inode)` with more than one link only once.
- Stay on the root's device. "Macintosh HD" is scanned at
  `/System/Volumes/Data` rather than `/`.
- Fold files under 1 MiB into a per-folder "smaller files" node, and drop the
  children of folders under 1 MiB, to bound memory.

## Consequences

- Totals line up with `du` and with what deleting a file actually frees.
- APFS clones are still counted per file; this is documented, and matches
  `du`.
- Tiny files can't be browsed individually, which is fine because they can't
  free meaningful space.
