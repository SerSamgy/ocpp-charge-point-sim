# Roadmap

Two tracks move in parallel: the OCPP protocol client and the hardware
and system side of the simulated charge point. Steps are numbered across
both tracks. Status: **done**, **in progress** or **not started**.

Each done item links to the pull request that finished it. Work done
before the repository existed came in with the migration
(`docs/decisions/0001-fresh-history.md`) and names its import commit
instead.

## Protocol track

1. **Done.** Python test CSMS (`csms/`), OCPP 1.6. Imported in
   `565ace1`.
2. **Done.** BootNotification and Heartbeat in the Rust client
   (`crates/cp-sim`); runs on the laptop and on the Pi. Imported in
   `2e2ecf2`.
3. **Done.** Protocol core.
   - [x] OCPP-J `Frame` (CALL, CALLRESULT, CALLERROR). Imported in
         `2e2ecf2`.
   - [x] typed messages (`Request` trait with action and response type).
         Imported in `2e2ecf2`.
   - [x] response matching by message ID. Imported in `2e2ecf2`.
   - [x] replies to server-initiated CALLs (CALLERROR `NotImplemented`).
         Imported in `2e2ecf2`.
   - [x] cargo workspace (done by the migration). Imported in `2e2ecf2`.
   - [x] **3e.** Retry BootNotification on `Rejected` / `Pending`,
         connection timeout, guard against `interval = 0`.
         [#5](https://github.com/SerSamgy/ocpp-charge-point-sim/pull/5)
   - [x] **3e2.** Reconnect with backoff after a lost connection
         (OCPP 2.0.1 part 4 §5.4 algorithm, `rand` for jitter); a failing
         Heartbeat or BootNotification CALL does not exit the program.
         [#6](https://github.com/SerSamgy/ocpp-charge-point-sim/pull/6)
   - [x] **3f.** Graceful shutdown on Ctrl+C and SIGTERM: Close 1001
         Going Away, bounded by a close timeout.
         [#10](https://github.com/SerSamgy/ocpp-charge-point-sim/pull/10)
4. **Done.** Connector state machine: Available → Preparing → Charging →
   Finishing → Available, plus Faulted. StatusNotification on every
   transition. Events come from the keyboard for now. Done in three pull
   requests.
   - [x] **4a.** The state machine (`connector`), the 1.6
         StatusNotification payloads, the mapping from state to message
         (`status16`) and the CSMS handler.
         [#11](https://github.com/SerSamgy/ocpp-charge-point-sim/pull/11)
   - [x] **4b-1.** The station keeps a record per connector and reports
         connector 0 and connector 1 after every `Accepted` and after
         every reconnect without BootNotification (errata 3.22). Each
         report carries the instant the status was reached. First check
         of StatusNotification from the Rust client against `just csms`.
         [#14](https://github.com/SerSamgy/ocpp-charge-point-sim/pull/14)
   - [x] **4b-2.** Keyboard events (`plug`, `start`, `stop`, `unplug`,
         `fault <kind>`, `clear`) from stdin on a thread of its own,
         stamped with the time of the key press; StatusNotification on
         every transition of a live connection; events that happen while
         offline are applied after the reconnect and reported once.
5. **Not started.** Transactions: Authorize, StartTransaction,
   StopTransaction, MeterValues with a simulated energy meter.

## Hardware and system track

6. **Done.** LED on GPIO via `rppal`. Imported in `f922c90`.
7. **Done.** Four buttons, debouncing, polling vs interrupts. See
   `docs/hardware/`. Imported in `f922c90`.
8. **Not started.** Join the tracks: buttons become state machine events
   via Tokio channels, LEDs show connector status. Requires 4, 6, 7.
9. **Not started.** Debug Probe and the serial console of the Pi over
   UART.
10. **Not started.** Autostart as a systemd service.

## Suggested order

8 → 5 → 10; 9 at any time.

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
      panics today). Done in step 3e,
      [#5](https://github.com/SerSamgy/ocpp-charge-point-sim/pull/5).
- [x] Retry BootNotification after `interval` on `Pending` / `Rejected`
      instead of exiting. Done in step 3e,
      [#5](https://github.com/SerSamgy/ocpp-charge-point-sim/pull/5).
- [x] A failing Heartbeat (timeout, CALLERROR) should not exit the program.
      Done in step 3e2,
      [#6](https://github.com/SerSamgy/ocpp-charge-point-sim/pull/6).
- [ ] Detect half-open connections with WebSocket ping
      (`WebSocketPingInterval`, OCPP-J 2.0.1 §8.4). Today a connection whose
      peer vanished without a TCP reset is noticed only when a write fails.
- [ ] Until WebSocket ping exists: treat N consecutive Heartbeat timeouts
      as a lost connection and reconnect, a cheap safety net.
- [ ] On shutdown, let a CALL in flight finish (up to its timeout)
      before closing; today it is cancelled (step 3f).
- [ ] A second Ctrl+C exits at once instead of waiting up to the close
      timeout (step 3f).
- [ ] Reply to the server's Close when the connection ends as lost:
      `read_loop` stops at the server's Close, and tungstenite sends the
      queued reply only on the next read, write or flush, so the server
      sees an abnormal closure (1006). `Client::close` already flushes it
      on shutdown and after Rejected (step 3f).
- [ ] `read_loop` logs every received Close as "server closed
      connection", also when it is the server's reply to our own Close.
- [x] After a reconnect without BootNotification, send StatusNotification
      for every connector again. Done in step 4b-1,
      [#14](https://github.com/SerSamgy/ocpp-charge-point-sim/pull/14).
- [ ] A keyboard command is checked against the connector state only
      when the session applies it, after Accepted, so the message for a
      command that does not fit can come late (after a reconnect, which
      can take hours after Rejected). Needs the state in the keyboard
      thread or a reply channel; a limitation until buttons replace the
      keyboard (step 8).
- [ ] `MinimumStatusDuration` (1.6 section 4.9): do not report a status
      that lasted less than the configured time.
- [ ] Retry a StatusNotification that failed on a live connection
      (timeout, CALLERROR): today it is logged and the CSMS may stay out
      of date until the next reconnect re-reports everything.
- [x] StatusNotification `timestamp`: whole seconds in UTC from the
      `time` crate (`timestamp`, `status16`). Done after step 4a,
      [#12](https://github.com/SerSamgy/ocpp-charge-point-sim/pull/12);
      the caller passes the instant in step 4b.
- [ ] Trust in `timestamp`: it comes from the system clock, which on a Pi
      without an RTC battery can be wrong until NTP has synchronised, and
      the CSMS believes it. Sync with `currentTime` of
      BootNotification/Heartbeat (1.6 section 4.2) or check NTP at deploy.
      The program start and the time of every status reached are fixed
      once, so a Pi that starts before NTP has synchronised (autostart,
      step 10) reports a wrong time for the untouched initial state on
      every report until the program restarts.
- [ ] StatusNotification `info` for an EV-side disconnect (`EV side
      disconnected`, 1.6 section 4.9, `StopTransactionOnEVSideDisconnect`):
      needs a reason in the Finishing state; today Unplug while Charging
      goes straight to Available.
- [ ] Other statuses: `SuspendedEV`, `SuspendedEVSE`, `Reserved`,
      `Unavailable`, and warning error codes such as `EVCommunicationError`
      together with a non-Faulted status.
- [ ] `ConnectionTimeOut` (Preparing → Available or Finishing, B1/B6).
- [ ] B6 (Preparing → Finishing) and F2 (Finishing → Preparing) are in the
      1.6 table but no event produces them; with the cable still plugged in
      after Finishing only Unplug works (step 5, where a new user at a
      plugged-in cable makes F2 natural).
- [ ] Faults in the connector state machine (step 8): plug and unplug are
      rejected while faulted, so the remembered pre-fault state may
      disagree with the real cable, and a second fault cause during Faulted
      is lost.
- [ ] Make the reconnect back-off settings (`RetryBackOffWaitMinimum`,
      `RetryBackOffRandomRange`, `RetryBackOffRepeatTimes`) configurable;
      today they are constants in `crates/cp-sim/src/main.rs`.
- [ ] Test that the client offers sub-protocol `ocpp1.6` in the
      WebSocket handshake: the fake CSMS in `test_support` answers
      `ocpp1.6` without checking the request (`docs/architecture.md`,
      invariants).
- [ ] Test the startup check that rejects a malformed or non-`ws(s)://`
      CSMS URL: it lives in `main`, so move it into a testable function
      first.
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
- [ ] Fix the six rustdoc warnings in `crates/cp-sim/src/frame.rs`
      (`<id>` and the like in doc comments read as unclosed HTML tags),
      then consider `cargo doc` with `RUSTDOCFLAGS=-D warnings` in
      `just ci`.
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
