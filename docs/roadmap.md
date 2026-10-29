# Roadmap

Two tracks move in parallel: the OCPP protocol client and the hardware
and system side of the simulated charge point. Steps are numbered across
both tracks. Status: **done**, **in progress** or **not started**.

## Protocol track

1. **Done.** Python test CSMS (`csms/`), OCPP 1.6.
2. **Done.** BootNotification and Heartbeat in the Rust client
   (`crates/cp-sim`); runs on the laptop and on the Pi.
3. **In progress.** Protocol core.
   - [x] OCPP-J `Frame` (CALL, CALLRESULT, CALLERROR)
   - [x] typed messages (`Request` trait with action and response type)
   - [x] response matching by message ID
   - [x] replies to server-initiated CALLs (CALLERROR `NotImplemented`)
   - [x] cargo workspace (done by the migration)
   - [x] **3e.** Retry BootNotification on `Rejected` / `Pending`,
         connection timeout, guard against `interval = 0`.
   - [ ] **3e2.** Reconnect with backoff after a lost connection
         (OCPP 2.0.1 part 4 §5.4 algorithm, `rand` for jitter); a failing
         Heartbeat or BootNotification CALL does not exit the program.
   - [ ] **3f.** Graceful shutdown on Ctrl+C.
4. **Not started.** Connector state machine: Available → Preparing →
   Charging → Finishing → Available, plus Faulted. StatusNotification on
   every transition. Events come from the keyboard for now.
5. **Not started.** Transactions: Authorize, StartTransaction,
   StopTransaction, MeterValues with a simulated energy meter.

## Hardware and system track

6. **Done.** LED on GPIO via `rppal`.
7. **Done.** Four buttons, debouncing, polling vs interrupts. See
   `docs/hardware/`.
8. **Not started.** Join the tracks: buttons become state machine events
   via Tokio channels, LEDs show connector status. Requires 4, 6, 7.
9. **Not started.** Debug Probe and the serial console of the Pi over
   UART.
10. **Not started.** Autostart as a systemd service.

## Suggested order

3e2 → 3f → 4 → 8 → 5 → 10; 9 at any time.

## Larger blocks after that

- **OCPP 2.0.1.** TransactionEvent instead of Start/StopTransaction,
  device model, an abstraction supporting both versions (the natural
  point to extract the `ocpp-proto` crate).
- **OCPP 2.1 (optional, after 2.0.1).** Extends 2.0.1, e.g. V2X.
- **Security profiles.** Basic auth over TLS, then client certificates
  (mTLS).
- **Relay as contactor.** Module choice postponed until the enclosure and
  fan are decided.
- **Control Pilot.** A microcontroller generating the IEC 61851 signal,
  connected to the Pi over UART.

## Postponed purchases

- Relay module.
- RTC battery for the Pi.

## Backlog

Found during the migration inventory; carried over as is
(see `docs/decisions/0002-as-is-migration.md`).

- [x] Handle `interval = 0` from the server (`tokio::time::interval`
      panics today). Done in step 3e.
- [x] Retry BootNotification after `interval` on `Pending` / `Rejected`
      instead of exiting. Done in step 3e.
- [ ] A failing Heartbeat (timeout, CALLERROR) should not exit the program.
      Part of step 3e2.
- [ ] Extract an `ocpp-proto` library crate (see `docs/archive/`).
- [ ] Turn `cp-blink` into a hardware library (pin table, `Button`,
      debounce) with the experiments as examples.
- [ ] Validate serialized messages against the official JSON schemas in
      tests.
- [ ] Structured logging with `tracing` instead of `println!`.
- [ ] Fix the test name typo `cpms` in `boot_response_parses_cpms_response`.
- [ ] `csms`: write the README and the `pyproject.toml` description.
- [ ] Configurable port for end-to-end checks: `csms/main.py` hard-codes
      port 9000 (`SERVER_PORT`), so a check fails whenever the port is
      taken, e.g. by a CSMS left running. Make the CSMS port configurable
      (environment variable or argument, also through `just csms`), and
      let the client read its URL the same way. Today the client takes
      the URL only as the first command-line argument, with
      `ws://localhost:9000/CP001` as the default.
- [ ] Let crates inherit `edition` from `[workspace.package]`.
- [ ] Agent permissions, follow-ups to
      `docs/decisions/0005-agent-permission-policy.md`:
  - [ ] evaluate enabling the Claude Code Bash sandbox (filesystem and
        network limits for shell commands);
  - [ ] close short-form gaps: `git push -fu`, `+refspec` pushes,
        `git branch -D`, `git switch -f`, `git commit -vn`;
  - [ ] ask or deny `rsync`, `sftp`, `uv remove|sync`;
  - [ ] check how the Code tab of Claude Desktop applies the rules
        (manual checklist in the pull request that added them).
- [ ] CI follow-ups, see `docs/decisions/0006-ci-on-github-actions.md`:
  - [ ] Dependabot: `.github/dependabot.yml` for `cargo`, `uv` and
        `github-actions`, commit prefix `build(deps):`;
  - [ ] CodeQL default setup;
  - [ ] end-to-end CI test cp-sim ↔ csms: needs a client mode that
        connects, completes Boot, sends one Heartbeat and exits;
  - [ ] OS-level sandboxing for the coding agent (the Bash sandbox, see
        the permission follow-ups above).
