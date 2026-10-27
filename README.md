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
just csms       # start the test CSMS on ws://0.0.0.0:9000/<charge_point_id>
just build-pi   # static aarch64 musl binaries in target/aarch64-unknown-linux-musl/release/
just deploy     # scp the cp-sim binary to $PI_HOST (default: cp-sim)
```

Run the client against a CSMS: `cargo run -p cp-sim -- ws://localhost:9000/CP001`.
