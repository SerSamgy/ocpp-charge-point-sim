# ocpp-charge-point-sim

A hobby OCPP 1.6 charge point simulator: a Rust client that runs on a
laptop or on a Raspberry Pi 5, hardware experiments with buttons and an LED
on the Pi, and a small Python test CSMS to talk to.

## Setup

Enable the commit message hook once per clone:

```sh
git config core.hooksPath .githooks
```

The Rust toolchain is pinned in `rust-toolchain.toml`; rustup installs it
(with the Raspberry Pi target) on first use.

Tools: [just](https://github.com/casey/just) as a command runner,
[cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) with Zig
for Pi builds, [uv](https://github.com/astral-sh/uv) for the CSMS.

## Layout

| Path | What |
|---|---|
| `crates/cp-sim` | OCPP 1.6 charge point client (Tokio, WebSocket) |
| `crates/cp-blink` | GPIO experiments on a Raspberry Pi 5: LED, buttons, debouncing |
| `csms/` | Python test CSMS based on the `ocpp` library |
| `docs/` | Roadmap, architecture decisions, hardware notes |

## Commands

```sh
just test       # Rust tests for the whole workspace
just ci         # everything CI runs, except the commit message check
just csms       # start the test CSMS on ws://0.0.0.0:9000/<charge_point_id>
just build-pi   # static aarch64 musl binaries in target/aarch64-unknown-linux-musl/release/
just deploy     # scp the cp-sim binary to $PI_HOST (default: cp-sim)
```

Run the client against a CSMS: `cargo run -p cp-sim -- ws://localhost:9000/CP001`.

## Coding agent permissions

Instructions for the coding agent live in `CLAUDE.md`.

`.claude/settings.json` is the shared project settings file of
[Claude Code](https://code.claude.com/docs/en/settings). It defines what the
coding agent may do in this repository: which commands run without asking
(build, test, local git, read-only `gh`), which need confirmation (push, PR
creation, dependency changes, deploy) and which are denied (merging,
force-push, skipping the commit hook, reading credentials).

Personal overrides go into `.claude/settings.local.json`, which is gitignored.
Rationale and known limitations: `docs/decisions/0005-agent-permission-policy.md`.

## Continuous integration

`.github/workflows/ci.yml` runs on every pull request and on pushes to
`main`. Three jobs:

| Job | Checks | Local |
|---|---|---|
| `rust` | `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo check` for `aarch64-unknown-linux-musl` | `just ci-rust` |
| `python` | `ruff check` and `ruff format --check` in `csms/` | `just ci-python` |
| `commits` | every non-merge commit of the PR passes `.githooks/commit-msg` | the hook itself |

The `rust` and `python` jobs call the same `just` recipes as `just ci`, so
local and CI checks cannot drift. Design and security choices:
`docs/decisions/0006-ci-on-github-actions.md`.
