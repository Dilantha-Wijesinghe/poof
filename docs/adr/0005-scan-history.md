# 5. Persist compact scan summaries to explain growth

- Status: accepted
- Date: 2026-10-16

## Context

The core user question is "where did my space go since last week?" A single
scan can't answer it. Storing whole trees would cost hundreds of MB.

## Decision

After each scan, save a `Summary`: every directory, and every file of at
least 32 MB, down to eight levels, keyed by relative path. Keep the 30 most
recent per scan root. Diff two summaries by attributing each change to the
deepest item that explains it, so reported changes sum to the real total.

## Consequences

- Each summary is a few hundred KB of JSON at most.
- Items that crossed the 32 MB threshold can be misreported by at most 32 MB.
  We only report changes of at least 100 MB.
- A free-space timeline (monitor samples) complements scan diffs by showing
  *when* space dropped, even between scans.
