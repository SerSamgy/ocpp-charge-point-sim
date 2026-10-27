# CLAUDE.md

## Project

An OCPP charge point simulator in Rust + Tokio. It runs on a laptop
and on a Raspberry Pi 5 acting as the "charge controller"; a small Python
CSMS (the OCPP server) is used for testing. This is a learning project:
the owner learns Rust, embedded Linux and hardware by reviewing
agent-written code and reading the explanations. Explanations matter as
much as the code.

The client speaks OCPP 1.6 today; 2.0.1 is planned (see the roadmap).
Do not hard-wire 1.6 where a second version will plug in.

Check protocol details against the specs, not memory; if the needed
spec is missing, say so.

- `docs/specs/<version>/` (local, gitignored) holds the spec PDFs, errata
  sheets and JSON schemas for each OCPP version.
- Errata take precedence over the main text.
- JSON schemas are the authority for message fields, types and limits.
  For OCPP 2.0.1, `appendices-csv/` is the authority for standard device
  model components and variables.
- `extra/` holds certification profiles and test cases: consult them
  only when a task needs them, not in routine searches.

Layout:

- `crates/cp-sim`: OCPP client (Tokio, WebSocket).
- `crates/cp-blink`: GPIO experiments on the Pi (LED, buttons).
- `csms/`: Python test CSMS (`uv`, `ruff`).
- `justfile`: all build, check and deploy commands (`just --list`).
- `docs/roadmap.md`: steps, statuses, backlog. Start every task here.
- `docs/decisions/`: ADRs (architecture decision records). Read the
  relevant ones before changing what they cover.
- `docs/hardware/`: pin table and wiring notes.

## Communication

- Chat messages and reports to the owner: Russian, informal "ты".
- Everything committed (code, comments, commit messages, PRs, docs except
  the journal): English.
- Every new technical term gets a short gloss on first use.
- Direct, no filler, no empty praise.
- Never invent facts about the owner or his setup; ask when unknown.

## Workflow

- One task = one branch = one PR, sized for one or two evenings. Branch
  name `<type>/<short-slug>`, e.g. `feat/boot-retry`.
- Non-trivial task: explore, propose a plan (plan mode when available),
  wait for approval before editing files. If the task clearly exceeds one
  or two evenings, propose splitting it.
- Out-of-scope findings go to the roadmap backlog, not into the PR.
- Small commits, one logical change each. The workspace builds and tests
  pass after every commit. Refactoring is separate from behaviour changes.
- Conventional Commits, validated by `.githooks/commit-msg`. The body
  explains why. End with a `Co-Authored-By` trailer for the model used.
- Every behaviour change comes with tests; tests are the specification.
- A new OCPP message also gets a handler in `csms/`; check it end-to-end
  locally against `just csms`.
- Run `just ci` before every push. Never use `--no-verify`.
- Push and PR creation need the owner's confirmation. Never merge, never
  force-push, never rewrite pushed history: the owner merges with merge
  commits and the journal references commit hashes.
- PR body in English: summary, why, how to verify.
- After opening a PR, wait for CI with `gh pr checks` and fix failures
  with new commits.
- Update `docs/roadmap.md` when a task changes its status. Write an ADR in
  `docs/decisions/` for decisions that are hard to reverse.

## Learning journal

- `docs/journal/` is private and gitignored. One entry per PR, in Russian,
  file `NNNN-short-slug.md` (next free number).
- Sections: Задача; Что сделано (with commit hashes); Зачем; Использованные
  возможности Rust и инструментов, с ловушками; Рассмотренные и отброшенные
  альтернативы; Новые термины; Как проверить самому.
- The journal is the main learning output. Explain Rust features and their
  pitfalls thoroughly, for a reader who knows Python and asyncio well but
  is a Rust beginner; compare with Python where it helps.
- Never reference journal entries from committed files.

## Final report

At the end of every task, in chat, in Russian: what was done, commits,
check results, deviations from the plan and why, open questions,
glossary.

## Rust conventions

- Cargo workspace, edition 2024, toolchain pinned in `rust-toolchain.toml`.
- `cargo fmt` and `cargo clippy -D warnings` are enforced by `just ci`.
- Pi builds: static `aarch64-unknown-linux-musl` via `cargo zigbuild`
  (`just build-pi`, ADR 0003).
- Prefer `#[expect(lint, reason = "...")]` over `#[allow]`.
- No `unwrap()`/`expect()` outside tests unless justified in a comment.

## Dependencies

- New or changed dependencies need the owner's confirmation.
- Justify each in the PR and the journal. Prefer well-maintained crates.
- Lock files are committed; CI uses `--locked`.

## Hardware and safety

- The agent never performs physical actions. The owner wires, powers and
  measures.
- For hardware tasks give wiring with both BCM and physical pin numbers,
  safety warnings, and what to check with the multimeter before powering.
- Pi GPIO is 3.3 V only and never tolerates 5 V. Never connect 5 V logic
  (e.g. an Arduino Uno) directly.
- No mains voltage in this project. If a task would involve it, stop and
  say so.
- Before running anything on the Pi (`just deploy`, `ssh`), say what will
  happen physically.

## Purchases

When something has to be bought, say exactly what, why, and an
approximate price in Germany. Never buy anything.

## Security

- Never read or print secrets. Private keys and certificates only as test
  material generated for this project (to be extended at the security
  profiles step).
- Never change git config; commits use the noreply address already set.
- `.claude/settings.json`, `justfile`, `.githooks/`, `.github/workflows/`
  are safety mechanisms. Change them only when the task requires it, and
  call it out in the PR.
