# 0003. Static musl builds for the Pi with cargo-zigbuild

## Context

The Rust code is developed on an x86_64 laptop and runs on a Raspberry
Pi 5 (aarch64). Building on the Pi is slow. A cross-compiled binary
linked against glibc must match the glibc version on the Pi, and a
cross linker for aarch64 has to come from somewhere.

## Decision

Cross-compile on the laptop for `aarch64-unknown-linux-musl` with
`cargo zigbuild --release`. Zig acts as the cross linker and ships the
musl C runtime, so no aarch64 GCC toolchain is needed. The result is a
single statically linked binary that is copied to the Pi with `scp` and
started directly. Release binaries are stripped (`strip = true`).

The commands live in the `justfile` (`build-pi`, `deploy`). No default
build target is set in `.cargo/config.toml`, so host builds and tests
keep working; none is needed because cargo-zigbuild configures the
linker itself. The toolchain and the musl target are pinned in
`rust-toolchain.toml`.

## Consequences

- No runtime dependencies on the Pi; any Linux on aarch64 can run it.
- The laptop needs `cargo-zigbuild` and `zig` installed.
- musl's allocator is slower than glibc's; irrelevant at this scale.
- `cargo build` without `--target` still builds for the host, so the Pi
  build must go through `just build-pi`.
