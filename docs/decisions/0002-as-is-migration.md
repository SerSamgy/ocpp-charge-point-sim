# 0002. Migrate the code as is

## Context

Moving three projects into one cargo workspace touches many files. The
inventory also found things worth improving: a panic on `interval = 0`,
no retry on a rejected boot, duplicated button tables, a test name typo.
Mixing such fixes into the move would make it hard to tell whether a
change in behaviour came from the migration or from a fix.

## Decision

Move the code to its new location with only the changes needed to build
in the workspace. No refactoring, no fixes, no renames. Every improvement
found along the way goes into `docs/roadmap.md` as a task.

## Consequences

- Each crate's source is byte-identical to the archived version, and the
  resolved dependency versions are unchanged.
- Known bugs are carried over on purpose; they are listed in the roadmap
  backlog.
- Some workspace features are not used yet (for example crates do not
  inherit `edition` from `[workspace.package]`); adopting them is
  follow-up work.
