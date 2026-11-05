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
| `csms/` | Python test CSMS on the `ocpp` library. Answers BootNotification (status sequence and interval configurable through `BOOT_STATUSES`, `BOOT_INTERVAL`), Heartbeat and StatusNotification (connector, status, error code and timestamp logged, empty result); logs the close code of a disconnected charge point. |

## Modules of `cp-sim`

Layered from the wire up; each module uses only the ones above it.

| Module | Responsibility | Depends on the OCPP version? |
|---|---|---|
| `frame` | OCPP-J RPC frame: CALL, CALLRESULT, CALLERROR as a JSON array. | No, same in 1.6 and 2.0.1. |
| `timestamp` | A UTC instant as RFC 3339 for the wire, whole seconds, with a `Z`; 1.6 and 2.0.1 accept the same text. Used by `status16`. | No. |
| `connector` | The state of one connector (Available, Preparing, Charging, Finishing, Faulted) and the events that change it; a pure state machine following the table of 1.6 section 4.9; `Connector` is the record the station keeps (number, state, instant the state was reached). `session` keeps the records and applies the events. | No: the state is our own enum, not the 1.6 wire status. |
| `keyboard` | Commands typed on the keyboard (`plug`, `start`, `stop`, `unplug`, `clear`, `fault <kind>`) as connector events, each stamped with the instant it was typed (`Input`); `parse` is pure, `spawn` reads stdin on a thread of its own. | No. |
| `ocpp16` | OCPP 1.6 payloads; the `Request` trait ties a payload to its action name and response type. | Yes. |
| `pending` | `PendingCalls`: outstanding CALLs by message ID, each waiting on a `oneshot` channel. | No. |
| `client` | One WebSocket connection: `Client::call` sends a typed request and waits for the typed answer; the reader task routes incoming frames; `Client::close` sends a Close frame with a status code. | Partly: sub-protocol `ocpp1.6`, `ocpp16::Request`. |
| `boot` | Maps a BootNotification response to the next step (Accepted, Pending, Rejected). | Yes, 1.6 section 4.2. |
| `status16` | Maps a connector state and the instant it was reached to a 1.6 StatusNotification request; `report` builds the list sent after Accepted and after a reconnect (errata 3.22): connector 0, then every connector. Connector 0 is not representable by `status_notification`. | Yes, through `connector` and `ocpp16`. |
| `backoff` | Wait before a reconnect, OCPP-J 2.0.1 part 4 section 5.4. | No, a transport rule. |
| `session` | Stays connected until a shutdown signal: connect, boot until Accepted, report the status of connector 0 and the connector, apply keyboard events and report each one, Heartbeat, reconnect; on the signal, close the connection and return. Owns the state that survives a connection (`Station`). | Yes, through `boot` and `ocpp16`. |
| `main` | Reads the CSMS URL, rejects a malformed one, fixes the settings, creates the input channel and starts the keyboard thread (`keyboard::spawn`), runs `session::run` until Ctrl+C or SIGTERM. | Boot payload is 1.6. |
| `test_support` | Tests only: `FakeCsms`, a WebSocket server driven by the test. | 1.6 sub-protocol. |

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
are logged and skipped.

Status reports: after Accepted and after every reconnect that skips the
boot, `session` sends the list from `status16::report` (connector 0, then
each connector) as ordinary CALLs, one after the other, because only one
CALL waits for an answer at a time.

Keyboard events: `main` starts the keyboard thread (`keyboard::spawn`),
which sends `Input`s over a bounded channel to `session::run`. `session`
reads them only after Accepted (until then they wait in the channel):
`keep_alive` applies the events that waited, in order, and reports the
final status once with the other reports, then applies each new event at
once and sends one StatusNotification for it. Every input carries the
instant it was typed, and the connector keeps it as `since`, so a report
shows when the status was reached. A command that does not fit the state
is printed and changes nothing; because the state is checked only when the
input is applied, the message can come late, after a reconnect. If stdin
ends, the sender is dropped and `session` stops listening to the channel
for good; the station keeps running.

Connection life cycle (`session`): connect → BootNotification until
Accepted → status reports (connector 0, then connector 1) → Heartbeat
every `interval` → on loss, wait (back-off or the Rejected interval) and
connect again. When the reader task ends, every
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
| Nothing but BootNotification goes out before Accepted. After Accepted and after every reconnect that skips the boot, connector 0 and then connector 1 are reported with the instant their status was reached (the fixed start time of the program, so far), not with the time of the report. | `session::accepted_reports_connector_0_then_1`, `session::reconnect_without_boot_reports_both_again`, `session::pending_boot_sends_no_report`, `session::connection_lost_during_report_reports_again` |
| A failed CALL (timeout, CALLERROR) on a live connection does not end the connection or the program. | `session::failed_boot_is_retried_on_same_connection`, `session::heartbeat_call_error_keeps_connection`, `session::heartbeat_timeout_keeps_connection`, `session::report_call_error_keeps_connection`, `session::report_timeout_keeps_connection` |
| Back-off grows while connecting fails and resets only after a successful OCPP exchange, not after a bare handshake. | `session::backoff_grows_while_connect_fails`, `session::handshake_alone_does_not_reset_backoff`, `session::successful_exchange_resets_backoff`, `session::successful_report_resets_backoff`, `backoff::doubles_up_to_repeat_times_then_stays` |
| Back-off arithmetic saturates instead of panicking on huge settings. | `backoff::huge_settings_saturate_instead_of_panicking` |
| On shutdown the client closes an open connection with 1001 Going Away and does not wait for a CALL in flight. | `session::shutdown_closes_connection_and_returns`, `session::shutdown_during_boot_call_closes_connection`, `session::shutdown_during_heartbeat_call_closes_connection`, `session::shutdown_during_report_closes_connection`, `session::shutdown_during_status_notification_closes_connection` |
| A keyboard event is applied only after Accepted. On a live connection a valid event sends one StatusNotification carrying the instant the event was typed; an event that does not fit sends nothing and changes nothing; events that waited are applied in order and reported once, with the instant of the last one. | `session::input_sends_status_notification`, `session::invalid_event_sends_nothing`, `session::fault_reports_the_error_code_and_clear_returns_to_the_pre_fault_state`, `session::inputs_wait_until_boot_is_accepted`, `session::inputs_while_offline_are_reported_once_with_the_time_of_the_last_one`, `session::connection_lost_during_an_input_report_reports_the_new_state_again` |
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
`status16` move into the version module.
