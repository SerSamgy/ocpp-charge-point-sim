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

# CI job `rust`: format, lints, tests, Pi target check, docs check
ci-rust:
    cargo fmt --check
    cargo clippy --locked --workspace --all-targets -- -D warnings
    cargo test --locked --workspace
    cargo check --locked --workspace --target aarch64-unknown-linux-musl
    just check-docs

# Check that every test named in the invariants table of the architecture map exists
check-docs doc="docs/architecture.md":
    #!/usr/bin/env bash
    set -euo pipefail
    # `--list` prints one `module::tests::name: test` line per test.
    tests=$(cargo test --locked -q -p cp-sim -- --list | sed -n 's/: test$//p')
    # Names in the last column of the table rows under "## Invariants".
    names=$(awk '/^## /{inside = ($0 == "## Invariants")} inside && /^\|/' "{{doc}}" \
        | awk -F'|' '{print $(NF-1)}' \
        | grep -oE '`[a-z0-9_]+::[a-z0-9_]+`' | tr -d '`' | sort -u || true)
    if [ -z "$names" ]; then
        echo "check-docs: no test names found in the invariants table of {{doc}}" >&2
        exit 1
    fi
    status=0
    for name in $names; do
        if ! grep -qxF "${name%%::*}::tests::${name#*::}" <<<"$tests"; then
            echo "check-docs: {{doc}} names \`$name\`, but cp-sim has no such test" >&2
            status=1
        fi
    done
    if [ "$status" -eq 0 ]; then
        echo "check-docs: all $(wc -w <<<"$names") tests named in {{doc}} exist"
    fi
    exit $status

# Build and test every commit of the current branch since `base`, each on a clean export
test-commits base="main":
    #!/usr/bin/env bash
    set -euo pipefail
    # A `git archive` export instead of a checkout: the working tree, the
    # index and HEAD stay untouched, and uncommitted changes cannot leak in.
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT
    # Shared between runs and commits, so only the first build is slow.
    export CARGO_TARGET_DIR="$PWD/target/test-commits"
    commits=$(git rev-list --reverse "{{base}}..HEAD")
    if [ -z "$commits" ]; then
        echo "test-commits: no commits between {{base}} and HEAD" >&2
        exit 1
    fi
    for sha in $commits; do
        echo "test-commits: $(git log -1 --format='%h %s' "$sha")"
        rm -rf "$work/src"
        mkdir "$work/src"
        # `-m`: give the files the time of extraction. `git archive` stores
        # the commit time, which is older than the last build in the shared
        # target directory, so cargo would take the sources for unchanged
        # and test the previous commit's binary again.
        git archive "$sha" | tar -x -m -C "$work/src"
        # From inside the export, so rustup picks that commit's toolchain.
        (cd "$work/src" && cargo test --locked --workspace --quiet)
    done
    echo "test-commits: all $(wc -w <<<"$commits") commits build and pass the tests"

# CI job `python`: ruff lint and format check for the test CSMS
ci-python:
    cd csms && uv run --locked ruff check
    cd csms && uv run --locked ruff format --check
