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
