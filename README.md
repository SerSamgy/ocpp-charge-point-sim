# ocpp-charge-point-sim

A hobby OCPP 1.6 charge point simulator: a Rust client that runs on a
laptop or on a Raspberry Pi 5, hardware experiments with buttons and an LED
on the Pi, and a small Python test CSMS to talk to.

## Build and run

You need [rustup](https://rustup.rs), [just](https://github.com/casey/just)
and [uv](https://github.com/astral-sh/uv); the Rust toolchain is pinned and
installs itself on the first `cargo` call.

```sh
just csms                                      # terminal 1: test CSMS on port 9000
cargo run -p cp-sim -- ws://localhost:9000/CP001   # terminal 2: the charge point
```

The client sends BootNotification, then Heartbeat, and reconnects when the
connection drops.

Before your first commit, follow **Setup** in
[`docs/development.md`](docs/development.md#setup): it enables the commit
message hook.

## Layout

| Path | What |
|---|---|
| `crates/cp-sim` | OCPP 1.6 charge point client (Tokio, WebSocket) |
| `crates/cp-blink` | GPIO experiments on a Raspberry Pi 5: LED, buttons, debouncing |
| `csms/` | Python test CSMS based on the `ocpp` library |
| `docs/` | Roadmap, architecture, development environment, decisions, hardware notes |

## Commands

```sh
just test       # Rust tests for the whole workspace
just ci         # everything CI runs, except the commit message check
just csms       # start the test CSMS on ws://0.0.0.0:9000/<charge_point_id>
just build-pi   # static aarch64 musl binaries in target/aarch64-unknown-linux-musl/release/
just deploy     # scp the cp-sim binary to $PI_HOST (default: cp-sim)
```

## Further reading

- [`docs/architecture.md`](docs/architecture.md): how the code is organised and what must stay true.
- [`docs/development.md`](docs/development.md): tools, cross-build, CI, coding agent settings, GitHub configuration.
- [`docs/roadmap.md`](docs/roadmap.md): steps, statuses, backlog.
- [`docs/decisions/`](docs/decisions/): architecture decision records.
- [`CLAUDE.md`](CLAUDE.md): instructions for the coding agent.
