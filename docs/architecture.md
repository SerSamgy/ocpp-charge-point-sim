# Architecture

A map of the code as it is now. Details that belong next to the code live
in the `//!` docs of each module; this page says how the pieces fit
together and what must stay true. A pull request that changes the
structure (crates, modules, message flow, invariants) updates this page.

## Crates and components

| Path | What |
|---|---|
| `crates/cp-sim` | The charge point: an OCPP-J client over WebSocket, Tokio runtime. One binary. |
| `crates/cp-blink` | GPIO experiments on the Pi (`rppal`): LED, buttons, debouncing. Standalone binaries, not used by `cp-sim` yet (roadmap step 8). Pins: `docs/hardware/`. |
| `csms/` | Python test CSMS on the `ocpp` library. Answers BootNotification (status sequence and interval configurable through `BOOT_STATUSES`, `BOOT_INTERVAL`), Heartbeat and StatusNotification (connector, status, error code and timestamp logged, empty result), Authorize, StartTransaction and StopTransaction (every idTag Accepted, except those in `BLOCKED_ID_TAGS`, Blocked in both, and in `START_REJECTED_ID_TAGS`, Blocked only in StartTransaction; compared without regard to case; transaction IDs from 1; a StopTransaction for an unknown or closed transaction gets an ordinary answer and a warning); logs the close code of a disconnected charge point. |

## Modules of `cp-sim`

Layered from the wire up; each module uses only the ones above it.

| Module | Responsibility | Depends on the OCPP version? |
|---|---|---|
| `frame` | OCPP-J RPC frame: CALL, CALLRESULT, CALLERROR as a JSON array. | No, same in 1.6 and 2.0.1. |
| `timestamp` | A UTC instant as RFC 3339 for the wire, whole seconds, with a `Z`; 1.6 and 2.0.1 accept the same text. Used by `status16`, and by `ocpp16` as the serializer of required time fields. | No. |
| `connector` | The state of one connector (Available, Preparing, Charging, Finishing, Faulted) and the events that change it; a pure state machine following the table of 1.6 section 4.9; `Connector` is the record the station keeps (number, state, instant the state was reached). `session` keeps the records and applies the events. | No: the state is our own enum, not the 1.6 wire status. |
| `meter` | A simulated energy meter: a register in watt-seconds that grows at a constant 7.4 kW while energy flows and reads in whole Wh; pure, instants passed in. `session` runs it exactly while the connector is Charging. | No. |
| `keyboard` | Commands typed on the keyboard (`plug`, `start [idTag]`, `stop`, `unplug`, `clear`, `fault <kind>`) as `Command`s, each stamped with the instant it was typed (`Input`); `start` presents an idTag (`DEMO-TAG` by default), kept as typed. No command stands for StartCharging. `parse` is pure, `spawn` reads stdin on a thread of its own. | No. |
| `ocpp16` | OCPP 1.6 payloads; the `Request` trait ties a payload to its action name and response type. | Yes. |
| `pending` | `PendingCalls`: outstanding CALLs by message ID, each waiting on a `oneshot` channel. | No. |
| `client` | One WebSocket connection: `Client::call` sends a typed request and waits for the typed answer; the reader task routes incoming frames; `Client::close` sends a Close frame with a status code. | Partly: sub-protocol `ocpp1.6`, `ocpp16::Request`. |
| `boot` | Maps a BootNotification response to the next step (Accepted, Pending, Rejected). | Yes, 1.6 section 4.2. |
| `transaction16` | 1.6 transactions: `IdTag` (1 to 20 characters), `Transaction` (ID from the CSMS, idTag, meter at the start), the Authorize, StartTransaction and StopTransaction requests, and what an answer means (`authorized`, `on_start_conf`: charge or stop with DeAuthorized; the `reason` and `idTag` of a stop). Pure. Behaves as if `StopTransactionOnInvalidId` and `StopTransactionOnEVSideDisconnect` were true. | Yes. |
| `status16` | Maps a connector state and the instant it was reached to a 1.6 StatusNotification request; `report` builds the list sent after Accepted and after a reconnect (errata 3.22): connector 0, then every connector. Connector 0 is not representable by `status_notification`. | Yes, through `connector` and `ocpp16`. |
| `backoff` | Wait before a reconnect, OCPP-J 2.0.1 part 4 section 5.4. | No, a transport rule. |
| `session` | Stays connected until a shutdown signal: connect, boot until Accepted, report the status of connector 0 and the connector, apply keyboard commands and report each one, start and stop transactions, Heartbeat, reconnect; on the signal, close the connection and return. Owns the state that survives a connection (`Station`: connector, meter, open transaction, undelivered StopTransaction). | Yes, through `boot`, `transaction16` and `ocpp16`. |
| `logging` | Installs the global `tracing` subscriber: events to stderr, filter from `RUST_LOG`, default `cp_sim=debug`; a malformed `RUST_LOG` is a startup error. Called first in `main`. | No. |
| `main` | Starts logging, reads the CSMS URL, rejects a malformed one, fixes the settings, creates the input channel and starts the keyboard thread (`keyboard::spawn`), runs `session::run` until Ctrl+C or SIGTERM. | Boot payload is 1.6. |
| `test_support` | Tests only: `FakeCsms`, a WebSocket server driven by the test; `LogCapture`, which collects the log events of the test's thread as text. | 1.6 sub-protocol. |

## Logging

The program logs through `tracing`, never `println!`: `clippy::print_stdout`
and `clippy::print_stderr` are denied for the crate. Every event is a level,
a message and fields (`connector = 1, from = Available, to = Preparing`),
not a pre-formatted string.

- `debug`: every frame on the wire, with `direction = "out"` or `"in"` and
  the frame text.
- `info`: the life of the connection, of the connector and of a transaction
  (connected, boot accepted, reconnecting, a connector changed state,
  transaction started with `transaction_id`, `id_tag`, `meter_start`,
  transaction stopped with `transaction_id`, `meter_stop`, `energy_wh`,
  `reason`, shutting down).
- `warn`: a failure that the program survives (a CALL that failed, a
  StopTransaction that will be retried, a malformed frame, no connection).

The replies to what the owner types on the keyboard are not log events and
stay `eprintln!` (`#[expect]` says so at each place), so a filter such as
`RUST_LOG=error` cannot hide them: help, an unknown command, a line that is
not UTF-8, an event that does not fit the connector state, and a `start`
that does not lead to charging (refused, failed, typed offline, invalid
idTag, previous stop not delivered).

Tests: `test_support::LogCapture` collects the events of the current thread.
It works through one global forwarding subscriber because `tracing` caches
per call site whether anybody listens, and tests run in parallel. A test
that reads the log must run on a `current_thread` runtime (the
`#[tokio::test]` default).

## Message flow

Outgoing CALL, for example Heartbeat:

1. `session` calls `Client::call(&HeartbeatRequest {})`.
2. `client` takes `call_lock`, makes a UUID message ID, builds `Frame::Call`,
   registers the ID in `pending` and writes the JSON text to the socket.
3. The reader task (`read_loop`) parses each text message into a `Frame`
   and hands it to `PendingCalls::complete`, which wakes the waiting call.
4. `Client::call` decodes the payload into `R::Response`, or returns a
   `CallFailure` (`Rejected`, `Timeout`, `Disconnected`, `Encode`, `Decode`).

Incoming CALL from the CSMS: `pending` does not claim it, so `read_loop`
answers with CALLERROR `NotImplemented`. Malformed or unexpected frames
are logged (`warn`) and skipped.

Status reports: after Accepted and after every reconnect that skips the
boot, `session` sends the list from `status16::report` (connector 0, then
each connector) as ordinary CALLs, one after the other, because only one
CALL waits for an answer at a time.

Keyboard commands: `main` starts the keyboard thread (`keyboard::spawn`),
which sends `Input`s over a bounded channel to `session::run`. `session`
reads them only after Accepted (until then they wait in the channel):
`keep_alive` applies the commands that waited, in order, and reports the
final status once with the other reports, then applies each new command at
once and sends one StatusNotification for it. Every input carries the
instant it was typed, and the connector keeps it as `since`, so a report
shows when the status was reached. A command that does not fit the state
is printed and changes nothing; because the state is checked only when the
input is applied, the message can come late, after a reconnect. A `start`
that waited is dropped with a message: there is no offline authorization.
If stdin ends, the sender is dropped and `session` stops listening to the
channel for good; the station keeps running.

Transactions (OCPP 1.6 sections 3.7, 4.1, 4.8, 4.10, errata 3.17):

1. `start [idTag]` in Preparing, on a live connection: Authorize. Anything
   but Accepted, or a failed CALL, ends here; the connector stays in
   Preparing.
2. StartTransaction with the meter reading and the instant of `start`.
   A failed or lost CALL ends here as well (a StartTransaction whose answer
   was lost may have created a transaction the station does not know,
   errata 3.18, step 5c).
3. With the answer, everything is applied before anything is sent: the
   `Transaction`, Charging (the meter starts), and, if the answer does not
   accept the idTag, the stop with reason DeAuthorized and Finishing. Then
   the frames go out: StatusNotification Charging, and for DeAuthorized
   StopTransaction and StatusNotification Finishing.
4. `stop` (Finishing, reason Local, so absent, with the idTag) or unplugging
   while charging (Available, reason EVDisconnected, no idTag) ends the
   transaction at once and puts its StopTransaction into `pending_stop`; the
   StopTransaction goes out before the StatusNotification of the change. A
   fault pauses the meter but keeps the transaction.
5. `pending_stop` is cleared only by the CSMS's answer. It goes out first
   after every reconnect, before the status reports, and after a CALLERROR
   or timeout again on every Heartbeat tick, whatever the Heartbeat's
   outcome. Until it is cleared, `start` is refused.

Connection life cycle (`session`): connect → BootNotification until
Accepted → the undelivered StopTransaction, if any → status reports
(connector 0, then connector 1) → Heartbeat every `interval` → on loss,
wait (back-off or the Rejected interval) and connect again. When the reader task ends, every
waiting and later CALL on that client fails with `Disconnected`, which is
how `session` learns that the connection is gone. `Station::serve` uses a
connection until it ends; `run` closes it after Rejected (status 1000) and
picks the wait.

Shutdown: `main` hands `session::run` a future that completes on Ctrl+C
or SIGTERM. `run` races it (`tokio::select!`) against connecting, serving
the connection and the wait before a reconnect. During `serve` the signal
cancels whatever is in flight, sends Close 1001 Going Away and waits for
the server's Close at most `close_timeout`; elsewhere `run` returns at
once. The close after Rejected is deliberately not raced against the
signal: cancelling it half way would start a second wait for the
shutdown close, so a signal during it lets it finish (at most
`close_timeout`) and stops `run` in the wait that follows. OCPP 1.6 and 2.0.1 define nothing for this, so the close codes come
from RFC 6455 section 7.4.1. The keyboard thread is a plain thread that is
never joined, not `tokio::io::stdin`: a pending read of that would keep the
runtime from stopping until Enter is pressed.

## Invariants

Each one is enforced by the tests named here (`module::tests::name`).
`just check-docs`, part of `just ci`, fails when a named test no longer
exists, so a test that is renamed or moved must be updated here too. It
does not check that a test still means what the row says; review does.

| Invariant | Tests |
|---|---|
| At most one outgoing CALL waits for an answer at a time (OCPP-J). | `client::only_one_call_in_flight` |
| A response is matched by message ID only; an unknown, late or repeated ID is dropped, and a CALL never completes a pending entry. | `client::call_times_out_and_late_response_is_ignored`, `client::ignores_malformed_and_unrelated_frames`, `pending::unknown_id_returns_frame_unchanged`, `pending::second_response_with_same_id_is_rejected`, `pending::call_frame_is_not_a_response` |
| Every CALL from the CSMS gets an answer (CALLERROR `NotImplemented` until handlers exist). | `client::answers_server_call_with_not_implemented` |
| Once the connection is gone, waiting and later CALLs fail at once with `Disconnected` instead of running into their timeout. | `client::calls_fail_at_once_when_connection_closes`, `client::close_ends_the_connection_and_fails_calls`, `pending::close_fails_waiting_and_later_calls` |
| `interval = 0` from the CSMS never reaches `tokio::time::interval` (it panics); a default is used. | `boot::accepted_with_zero_interval_uses_default_heartbeat`, `boot::pending_with_zero_interval_uses_default_retry`, `boot::rejected_with_zero_interval_uses_default_retry` |
| Pending boots again on the same connection; Rejected closes it and boots again on a new one. | `session::pending_boots_again_on_same_connection_after_interval`, `session::rejected_reconnects_after_interval_and_boots_again` |
| After Accepted, a reconnect does not send BootNotification again. | `session::after_accepted_reconnects_without_boot`, `session::connection_lost_while_pending_boots_again` |
| Nothing but BootNotification goes out before Accepted. After Accepted and after every reconnect that skips the boot, connector 0 and then connector 1 are reported with the instant their status was reached, not with the time of the report; only an undelivered StopTransaction goes out before them. | `session::accepted_reports_connector_0_then_1`, `session::reconnect_without_boot_reports_both_again`, `session::pending_boot_sends_no_report`, `session::connection_lost_during_report_reports_again`, `session::stop_while_offline_is_sent_first_after_the_reconnect` |
| A failed CALL (timeout, CALLERROR) on a live connection does not end the connection or the program. | `session::failed_boot_is_retried_on_same_connection`, `session::heartbeat_call_error_keeps_connection`, `session::heartbeat_timeout_keeps_connection`, `session::report_call_error_keeps_connection`, `session::report_timeout_keeps_connection` |
| Back-off grows while connecting fails and resets only after a successful OCPP exchange, not after a bare handshake. | `session::backoff_grows_while_connect_fails`, `session::handshake_alone_does_not_reset_backoff`, `session::successful_exchange_resets_backoff`, `session::successful_report_resets_backoff`, `backoff::doubles_up_to_repeat_times_then_stays` |
| Back-off arithmetic saturates instead of panicking on huge settings. | `backoff::huge_settings_saturate_instead_of_panicking` |
| The meter never runs backwards and saturates instead of panicking. | `meter::a_clock_set_back_counts_as_zero`, `meter::huge_periods_saturate_instead_of_panicking` |
| An idTag has 1 to 20 characters, counted as characters, and keeps its case. | `transaction16::an_id_tag_has_one_to_twenty_characters`, `transaction16::the_length_counts_characters_not_bytes`, `transaction16::an_id_tag_keeps_its_case`, `keyboard::start_takes_an_id_tag_and_keeps_its_case` |
| Every frame on the wire is logged at `debug` with its direction (`out` or `in`) and text. | `client::frames_are_logged_with_their_direction` |
| A connector state change is logged at `info` with the connector, the old and the new state as fields. | `session::state_change_is_logged_with_its_fields` |
| `RUST_LOG` overrides the default filter; blank means the default; a malformed value stops the program at startup and names the value. | `logging::default_filter_logs_debug_of_this_crate`, `logging::blank_value_means_the_default`, `logging::rust_log_overrides_the_default`, `logging::malformed_directives_are_an_error` |
| On shutdown the client closes an open connection with 1001 Going Away and does not wait for a CALL in flight. An open transaction is not stopped and an undelivered StopTransaction is lost (backlog). | `session::shutdown_closes_connection_and_returns`, `session::shutdown_during_boot_call_closes_connection`, `session::shutdown_during_heartbeat_call_closes_connection`, `session::shutdown_during_report_closes_connection`, `session::shutdown_during_status_notification_closes_connection`, `session::shutdown_during_authorize_closes_connection`, `session::shutdown_during_start_transaction_closes_connection`, `session::shutdown_during_stop_transaction_closes_connection` |
| A keyboard command is applied only after Accepted. On a live connection a valid command other than `start` sends one StatusNotification carrying the instant it was typed (after the StopTransaction, if it ends a transaction); a command that does not fit sends nothing and changes nothing; commands that waited are applied in order and reported once, with the instant of the last one. | `session::input_sends_status_notification`, `session::invalid_event_sends_nothing`, `session::fault_reports_the_error_code_and_clear_returns_to_the_pre_fault_state`, `session::inputs_wait_until_boot_is_accepted`, `session::inputs_while_offline_are_reported_once_with_the_time_of_the_last_one`, `session::connection_lost_during_an_input_report_reports_the_new_state_again` |
| The connector charges only in a transaction the CSMS has answered, and only for an idTag it authorized: `start` sends Authorize, then StartTransaction, then StatusNotification Charging; a refused idTag, a `start` outside Preparing, an invalid idTag or a `start` typed offline sends nothing more. | `session::start_authorizes_then_starts_the_transaction_then_charges`, `session::a_refused_id_tag_starts_nothing`, `session::start_outside_preparing_sends_nothing`, `session::an_invalid_id_tag_sends_nothing`, `session::start_while_offline_is_ignored`, `transaction16::only_accepted_is_authorized` |
| A failed or lost Authorize or StartTransaction starts no transaction and leaves the connector in Preparing; a CALLERROR or timeout keeps the connection. | `session::connection_lost_during_authorize_starts_no_transaction`, `session::connection_lost_during_start_transaction_starts_no_transaction`, `session::authorize_call_error_keeps_connection`, `session::authorize_timeout_keeps_connection`, `session::start_transaction_call_error_keeps_connection`, `session::start_transaction_timeout_keeps_connection` |
| `stop` ends the transaction with the idTag and no reason, unplugging while charging with reason EVDisconnected; a fault keeps it. `meterStop` is the meter at that instant: energy flows only while Charging. | `session::stop_ends_the_transaction_with_the_meter_reading`, `session::unplug_while_charging_ends_the_transaction_as_ev_disconnected`, `session::a_fault_keeps_the_transaction_and_its_time_is_not_charged`, `session::the_transaction_survives_a_reconnect`, `transaction16::a_stop_by_the_user_carries_the_id_tag_and_no_reason`, `transaction16::a_stop_by_the_station_has_a_reason_and_no_id_tag`, `meter::a_pause_adds_nothing_and_fractions_are_kept` |
| A StartTransaction.conf that does not accept the idTag stops the transaction with reason DeAuthorized; the stop is applied before any report is sent, so a lost connection cannot leave it running. | `session::a_start_conf_not_accepted_stops_the_transaction_as_deauthorized`, `session::connection_lost_after_a_deauthorized_start_still_stops_the_transaction`, `transaction16::a_start_conf_not_accepted_stops_the_transaction` |
| A StopTransaction stays until the CSMS answers it: after a lost connection it goes out first on the next one; after a CALLERROR or timeout the status still goes out and the StopTransaction again on every Heartbeat tick, even when the Heartbeat fails; until then `start` is refused. | `session::connection_lost_during_stop_transaction_sends_it_again`, `session::stop_while_offline_is_sent_first_after_the_reconnect`, `session::stop_transaction_call_error_is_retried_after_the_heartbeat`, `session::stop_transaction_timeout_is_retried_after_the_heartbeat`, `session::stop_transaction_is_retried_even_when_the_heartbeat_fails`, `session::start_is_refused_until_the_stop_is_delivered` |
| A transaction start and stop are logged at `info` with their fields. | `session::a_started_transaction_is_logged_with_its_fields`, `session::a_stopped_transaction_is_logged_with_its_fields` |
| The end of the keyboard input does not stop the station or make it spin. | `session::heartbeats_continue_after_the_sender_is_dropped` |
| The keyboard reader skips a bad line, also one that is not UTF-8, and ends on a read error or when the session is gone. | `keyboard::a_line_that_is_not_utf8_is_skipped_and_reading_goes_on`, `keyboard::a_read_error_ends_the_reader`, `keyboard::a_dropped_receiver_ends_the_reader_after_the_first_line` |
| Shutdown never waits for a back-off, the wait after Rejected or a connect timeout. | `session::shutdown_during_backoff_returns_at_once`, `session::shutdown_during_rejected_wait_returns_at_once`, `session::shutdown_during_connect_returns_at_once` |
| `disconnect` never awaits a reader task that was already awaited to the end (that panics). | `session::disconnect_after_reader_was_awaited_does_not_panic` |
| Shutdown ends within `close_timeout` even if the server never answers the Close. The timeout also covers sending the Close; a send that hangs on a full socket buffer is not tested, as a small Close frame always fits into the kernel buffer. | `session::shutdown_gives_up_after_close_timeout` |
| Closing a connection that is already closing is not an error; a Close from the server gets a Close in reply (RFC 6455 section 5.5.1). | `client::second_close_is_ok`, `client::close_after_server_closed_is_ok` |
| A connector makes only the transitions of the 1.6 table (section 4.9); a fault returns to the pre-fault state. | `connector::no_transition_outside_the_16_table`, `connector::fault_from_every_state_remembers_it_and_clear_fault_resumes` |
| An event that does not fit the state is returned as an error and leaves the state unchanged. | `connector::invalid_events_leave_state_unchanged` |
| The client offers sub-protocol `ocpp1.6` in the handshake. | None yet: the fake CSMS answers `ocpp1.6` without looking at the request. Backlog. |
| A malformed or non-`ws(s)://` URL stops the program at startup instead of being retried forever. | None yet: the check lives in `main`. Backlog. |

## Extension points for OCPP 2.0.1

1.6 is wired in at three places today: the `ocpp16` module, the
sub-protocol header in `Client::connect`, and the `ocpp16::Request` bound
of `Client::call`. `frame`, `pending`, `backoff` and `timestamp` are
already version-independent.

The plan (roadmap, "Larger blocks"): extract the version-independent
parts and the `Request` trait into an `ocpp-proto` library crate (an
earlier attempt is kept in `docs/archive/`), add a `v201` message module
beside `v16`, let the version pick the sub-protocol, and give `session`
a per-version boot and status logic. Behaviour that differs between
versions (for example `boot::next_step`, which follows 1.6 section 4.2)
belongs to the version module, not to `session`. The same goes for
`status16`: 2.0.1 gets its own mapping from the shared `connector` state
(Preparing, Charging and Finishing become `Occupied`), and `boot` and
`status16` move into the version module. The transaction state in
`Station` (`Transaction`, `pending_stop`, both from `transaction16`) is
1.6 too: 2.0.1 replaces it with its own, built on TransactionEvent, where
the station chooses a string `transactionId`. `meter` and the shared
`connector` state stay.
