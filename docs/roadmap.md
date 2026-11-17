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
         [#15](https://github.com/SerSamgy/ocpp-charge-point-sim/pull/15)
5. **Done.** Transactions: Authorize, StartTransaction,
   StopTransaction, MeterValues with a simulated energy meter. In three
   pull requests.
   - [x] **5a.** Simulated energy meter (constant 7.4 kW while Charging),
         `start [idTag]` with Authorize and StartTransaction, `stop` and
         unplug with StopTransaction, DeAuthorized stop (errata 3.17), an
         undelivered StopTransaction kept and retried (`pending_stop`), CSMS
         handlers.
         [#17](https://github.com/SerSamgy/ocpp-charge-point-sim/pull/17)
   - [x] **5b.** Periodic MeterValues during a transaction
         (`MeterValueSampleInterval`, 10 s, errata 3.15) with the energy
         register; no `transactionData` in StopTransaction (as if
         `StopTxnSampledData` and `StopTxnAlignedData` were empty, section
         3.16.5).
         [#18](https://github.com/SerSamgy/ocpp-charge-point-sim/pull/18)
   - [x] **5c1.** A queue for StopTransaction and MeterValues with
         `TransactionMessageAttempts` (3) and
         `TransactionMessageRetryInterval` (10 s), section 3.7.1; discard
         after the last attempt; at most 30 samples wait. Replaces
         `pending_stop`. `start` waits for an empty queue.
         [#19](https://github.com/SerSamgy/ocpp-charge-point-sim/pull/19)
   - [x] **5c2.** StartTransaction through the queue: the transaction
         starts on the station right after Authorize Accepted, and its
         messages carry `transactionId` -1 if StartTransaction was never
         answered (errata 3.18). This changes the 5a model, where a
         transaction exists only once StartTransaction is answered, and the
         5a tests that rely on it. To settle there: a StartTransaction.conf
         that is not Accepted may come late, in any connector state,
         including Faulted, where clearing the fault must not resume
         charging without a transaction; a retried StartTransaction may
         open a second transaction in the CSMS if only the answer was lost;
         the order of status reports around a late DeAuthorized; one place
         for the transaction ID. Removes the 5c1 wait of `start` for an
         empty queue, which can last up to about an hour behind a failing
         CSMS. Done: a deauthorization stops the transaction at the time of
         the answer, a fault then clears to Finishing, and the test CSMS
         answers a repeated start with the same ID.
         [#20](https://github.com/SerSamgy/ocpp-charge-point-sim/pull/20)
   - [x] **5c3.** MeterValues for the sample points that fall while the
         station is offline (section 3.7): the sampler follows a grid in
         UTC from the start of the transaction (`sample_grid`), and the
         points of the offline time are queued when the waiting inputs are
         applied after the reconnect, each read with the commands entered
         before it only. Every missed point gets its own sample (it was one
         for all of them); `sample_queue_limit` 30 → 1440 (four hours at
         10 s).
         [#21](https://github.com/SerSamgy/ocpp-charge-point-sim/pull/21)

## Hardware and system track

Every step of this track follows `docs/hardware/protocol.md`; its plan
contains a checklist for the step, with sources.

6. **Done.** LED on GPIO via `rppal`. Imported in `f922c90`.
7. **Done.** Four buttons, debouncing, polling vs interrupts. See
   `docs/hardware/`. Imported in `f922c90`.
8. **Not started.** Join the tracks: buttons become state machine events
   via Tokio channels, LEDs show connector status. Requires 4, 6, 7.
9. **Not started.** Debug Probe and the serial console of the Pi over
   UART.
10. **Not started.** Autostart as a systemd service.

## Suggested order

8 → 9 → 10 (step 5 is done). 9 comes before 10: the serial console is a
way into the Pi when an autostart service or a boot file breaks it, and
the Pi has a single SD card.

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
      OCPP-J 1.6 errata 3.11 recommends a `WebSocketPingInterval` below
      `TransactionMessageAttempts` × `TransactionMessageRetryInterval`, so
      a half-open connection is found before a queued message is discarded
      (step 5c1).
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
- [ ] `read_loop` logs every received Close as "server closed the
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
- [ ] Stop an open transaction on shutdown (Ctrl+C, SIGTERM) with a
      StopTransaction (reason `Local` or `Reboot`); today the CSMS keeps
      it open (step 5a).
- [ ] The queue of transaction-related messages is lost on shutdown, so a
      transaction that already ended may never reach the CSMS, and since
      step 5c2 not even its StartTransaction, although energy was
      delivered: send the queue before Close, or keep it on disk (steps 5a,
      5c1, 5c2).
- [ ] A message on which the CSMS drops the connection blocks the queue:
      a lost connection does not count as a failed attempt (step 5c1).
- [ ] Thin out the samples of a long outage instead of dropping the
      oldest ones when `sample_queue_limit` is reached (for example keep
      every second one): the CSMS would see the whole outage at a coarser
      resolution. Needs a rule in `TxQueue` that copes with a head waiting
      for a retry (step 5c3).
- [ ] Send the queue one message per loop iteration instead of in one go
      (`drain`): after a long outage up to `sample_queue_limit` samples go
      out before the status reports, and neither keyboard input nor
      Heartbeat is polled meanwhile (about 2.4 minutes for 1440 samples at
      100 ms per CALL) (step 5c3).
- [ ] A CSMS that keeps failing MeterValues holds back a StopTransaction
      for up to about 12 h (CALLERROR) or 48 h (no answer) at
      `sample_queue_limit` 1440. A rule could drop the samples of a
      transaction that wait before its Stop once one of them was
      discarded (step 5c3).
- [ ] A StopTransaction can carry an earlier time and a lower `meterStop`
      than a sample already taken: the keyboard thread stamps `at` before
      it sends, so a sample may be taken for a point after it, and a clock
      set back does the same (step 5c3).
- [ ] Retry a transaction message with the same message ID (OCPP-J 1.6
      errata 3.1 allows it), so a CSMS can tell a retry from a new message;
      today every CALL gets a new UUID (step 5c1).
- [ ] Offline authorization: Authorization Cache, Local Authorization List,
      `AllowOfflineTxForUnknownId` (section 3.5). Today a `start` typed
      while offline is dropped (step 5a).
- [ ] Use `idTagInfo` beyond its status: `expiryDate`, `parentIdTag`
      (section 3.10), `ConcurrentTx`.
- [ ] Configuration keys (`GetConfiguration`, `ChangeConfiguration`):
      `StopTransactionOnInvalidId`, `StopTransactionOnEVSideDisconnect`,
      `MeterValueSampleInterval` are constants today (steps 5a, 5b). An
      interval from the CSMS must be checked like `interval = 0`: zero and
      huge values already turn sampling off instead of panicking.
- [ ] `transactionData` in StopTransaction (`StopTxnSampledData`,
      `StopTxnAlignedData`; errata 3.14 recommends the Transaction.Begin and
      Transaction.End values of every configured measurand). Step 5b leaves
      it out.
- [ ] Clock-aligned meter values (`ClockAlignedDataInterval`,
      `MeterValuesAlignedData`, section 3.16.2), for example every 15
      minutes from midnight.
- [ ] More measurands in MeterValues (`Power.Active.Import`,
      `Current.Import`, `Voltage`, `SoC`), phases and locations, and the main
      meter on connector 0; today a sample carries only the energy register.
- [ ] Real energy measurement instead of the constant 7.4 kW of `meter`:
      a current and voltage sensor on the Pi (hardware track).
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
      Since step 5c3 the sample grid is also tied to the wall clock, where
      the tokio interval it replaced was monotonic: a clock stepped forward
      during a transaction gives a burst of up to `sample_queue_limit`
      samples with invented times, one stepped back a pause of the same
      length.
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
      after Finishing only Unplug works. Step 5a left it so: `start` in
      Finishing is refused; a new user at a plugged-in cable would make F2
      natural.
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
- [ ] Add the physical header pin numbers to the pin table in
      `docs/hardware/README.md` (`CLAUDE.md` asks for BCM and physical
      numbers; the table has BCM only). Check them against the official
      Raspberry Pi documentation.
- [ ] Hardware protocol (`docs/hardware/protocol.md`), to settle in the
      first hardware pull request, against the official documentation, and
      to record in `docs/hardware/` with links:
  - [ ] how to tell that the Pi 5 is powered off (the LEDs after a
        shutdown, the 5 V pin while the cable is in);
  - [ ] step 9: which connector and which UART the console of the Pi 5
        and the Debug Probe use, whether `config.txt` needs a change (a
        class P change), and the device name on the laptop;
  - [ ] whether a tty echoes what it receives so that a plain `cat` of a
        console would write into it (`stty -echo raw` first);
  - [ ] what `rppal` does with a pin when the process is killed by a
        signal (it resets pins on `Drop`);
  - [ ] whether the first user of the Pi has `sudo` without a password;
  - [ ] the checklist for each step (8, 9, 10, relay, Control Pilot) is
        written in the plan of that step, with sources.
- [ ] Turn `cp-blink` into a hardware library (pin table, `Button`,
      debounce) with the experiments as examples.
- [ ] Validate serialized messages against the official JSON schemas in
      tests.
- [x] Structured logging with `tracing` instead of `println!`. Done in
      `cp-sim`, see `docs/architecture.md` (Logging),
      [#16](https://github.com/SerSamgy/ocpp-charge-point-sim/pull/16).
- [ ] Logging follow-ups: a span with the charge point ID around the
      session, a JSON output format for journald and log collectors on the
      Pi, `tracing` instead of `println!` in `cp-blink`.
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
  - [ ] ask or deny `rsync`, `sftp`, `uv remove|sync`; for the hardware
        protocol (`docs/hardware/protocol.md`) also ask for `picocom`,
        `minicom`, `screen`, `tio`, `stty`, `dd`, `ssh-keygen`,
        `ssh-copy-id` and the bare `just deploy` (the rule `just deploy *`
        may not match it), and check whether `Read(//dev/**)` should ask
        or be denied (what Claude Code does for a read outside the
        project is not verified). An ask rule beats a broad allow in
        `.claude/settings.local.json`, which is where it pays off;
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
