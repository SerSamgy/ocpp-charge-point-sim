# 0006. CI on GitHub Actions: strict, minimal permissions, pinned actions

## Context

CI is the main feedback loop for agent-written code, so it has to be
strict, fast and safe. `main` is protected by a ruleset (changes only via
pull request), the repository is public, and the toolchain is pinned in
`rust-toolchain.toml`.

## Decision

One workflow, `.github/workflows/ci.yml`, on `pull_request` and on `push`
to `main`. Three jobs with short, stable names, meant to become required
checks: `rust`, `python`, `commits`.

- **Same commands locally and in CI.** The jobs run `just ci-rust` and
  `just ci-python`; `just ci` runs both. The workflow does not repeat the
  command lines, so they cannot drift.
- **Strict.** `cargo clippy ... -- -D warnings`, `cargo fmt --check`,
  `ruff check`, `ruff format --check`, and a `cargo check` for the Pi
  target `aarch64-unknown-linux-musl`.
- **Toolchain.** `rustup toolchain install --profile minimal` reads
  `rust-toolchain.toml` (version, components, target). The runner image
  ships a different Rust patch version, so the pin is installed explicitly.
- **Minimal permissions.** Top-level `permissions: contents: read`; no job
  overrides it. Checkout runs with `persist-credentials: false`.
- **Pinned actions.** Every action is pinned to a full commit SHA with the
  released tag in a trailing comment. Tags are mutable; a SHA is not. SHAs
  are resolved with `git ls-remote` against the action's own repository,
  and for annotated tags the peeled commit (`^{}`) is used.
- **No `pull_request_target`.** It runs with the base repository's
  privileges; `pull_request` is enough because the jobs need no secrets and
  no write access.
- **`commits` reuses the hook.** It runs `.githooks/commit-msg` on the
  message of each non-merge commit of `base..head`. The rules live in one
  place; the local hook can be bypassed, the CI check cannot.
- **Concurrency.** Runs of the same PR cancel each other; pushes to `main`
  are never cancelled (the group falls back to the run id).
- **Timeouts.** Every job has `timeout-minutes`.

## Consequences

- Two third-party actions beyond checkout (`Swatinem/rust-cache`,
  `astral-sh/setup-uv`) plus `extractions/setup-just`; SHAs are updated by
  hand until Dependabot is set up (roadmap).
- The workflow file is protected by an ask rule for the coding agent.
- Not covered yet: an end-to-end test of cp-sim against the CSMS, CodeQL,
  dependency updates (all in the roadmap).
