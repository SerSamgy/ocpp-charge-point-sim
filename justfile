# SSH host of the Raspberry Pi; override with PI_HOST=... in the environment.
PI_HOST := env("PI_HOST", "cp-sim")

# List recipes
default:
    @just --list

# Run Rust tests for the whole workspace
test:
    cargo test --workspace

# Cross-compile static release binaries for the Raspberry Pi
build-pi:
    cargo zigbuild --release --workspace --target aarch64-unknown-linux-musl

# Copy the cp-sim binary to the Pi
deploy:
    scp target/aarch64-unknown-linux-musl/release/cp-sim {{PI_HOST}}:~/

# Start the Python test CSMS on port 9000
csms:
    cd csms && uv run main.py

# Everything CI runs on a pull request, except the commit message check
ci: ci-rust ci-python

# CI job `rust`: format, lints, tests, Pi target check
ci-rust:
    cargo fmt --check
    cargo clippy --locked --workspace --all-targets -- -D warnings
    cargo test --locked --workspace
    cargo check --locked --workspace --target aarch64-unknown-linux-musl

# CI job `python`: ruff lint and format check for the test CSMS
ci-python:
    cd csms && uv run --locked ruff check
    cd csms && uv run --locked ruff format --check
