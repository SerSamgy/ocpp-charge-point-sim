# 0001. Start the repository with fresh history

## Context

The project grew as three separate local repositories: the Rust client
(`cp-sim`, a few commits plus an abandoned stash), the GPIO experiments
(`cp-blink`, one commit) and the Python test CSMS (`csms`, no commits).
They are being merged into one public repository. The old commits carry
a personal email address and their history is short and experimental.

## Decision

Create the new repository from scratch. No history is imported. The old
folders stay untouched on disk as an archive; the one piece of their
history worth keeping, the unfinished `ocpp-proto` workspace attempt, is
exported as a patch to `docs/archive/`.

## Consequences

- The public history starts with a clean, consistently formatted set of
  commits under a GitHub noreply address.
- `git blame` and `git log` do not reach back before the migration; the
  archive folders are the reference for earlier states.
- No history rewriting tools are needed and nothing private can leak
  through old commits.
