# Roadmap

Two tracks move in parallel: the OCPP protocol client and the hardware
of the simulated charge point. Steps are numbered across both tracks.

## OCPP track

### Done

- **Step 1.** Python test CSMS (`csms/`), OCPP 1.6, BootNotification and
  Heartbeat.
- **Step 2.** Rust client (`crates/cp-sim`) with BootNotification and
  Heartbeat; runs on the laptop and on the Pi.

### In progress

- **Step 3.** Robust client.
  - [x] typed serde messages (`Request` trait with action and response type)
  - [x] response matching by message ID
  - [x] CALLERROR handling
  - [ ] reconnect with backoff
  - [ ] clean shutdown on Ctrl+C

### Next

- Finish step 3.

## Hardware track

### Done

- **Step 6.** LED on GPIO blinking from Rust via `rppal`.
- **Step 7.** Four buttons as simulator events, debouncing, polling vs
  interrupts. See `docs/hardware/`.

### In progress

- Nothing.

### Next

- **Step 8.** Button events into Tokio alongside the OCPP client.

## Backlog

Found during the migration inventory; carried over as is
(see `docs/decisions/0002-as-is-migration.md`).

- [ ] Handle `interval = 0` from the server (`tokio::time::interval`
      panics today).
- [ ] Retry BootNotification after `interval` on `Pending` / `Rejected`
      instead of exiting.
- [ ] A failing Heartbeat (timeout, CALLERROR) should not exit the program.
- [ ] Extract an `ocpp-proto` library crate (see `docs/archive/`).
- [ ] Turn `cp-blink` into a hardware library (pin table, `Button`,
      debounce) with the experiments as examples.
- [ ] Structured logging with `tracing` instead of `println!`.
- [ ] Fix the test name typo `cpms` in `boot_response_parses_cpms_response`.
- [ ] `csms`: write the README and the `pyproject.toml` description.
- [ ] Validate commit messages in CI too (the local hook can be bypassed).
- [ ] Let crates inherit `edition` from `[workspace.package]`.
- [ ] Agent permissions, follow-ups to
      `docs/decisions/0005-agent-permission-policy.md`:
  - [ ] evaluate enabling the Claude Code Bash sandbox (filesystem and
        network limits for shell commands);
  - [ ] ask before editing `justfile`, `.githooks/**` and `build.rs`
        (allowed commands run them);
  - [ ] close short-form gaps: `git commit -n`, `git push -fu`,
        `+refspec` pushes, `git branch -D`, `git switch -f`;
  - [ ] ask or deny `curl`, `wget`, `rsync`, `sftp`, `uv remove|sync`;
  - [ ] check how the Code tab of Claude Desktop applies the rules
        (manual checklist in the pull request that added them).

## Later scope

- OCPP 2.0.1.
- TLS and client certificates (OCPP security profiles).
- A relay as the contactor.
- A microcontroller generating the IEC 61851 Control Pilot signal,
  connected to the Pi over UART.
