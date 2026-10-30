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
| `csms/` | Python test CSMS on the `ocpp` library. Answers BootNotification (status sequence and interval configurable through `BOOT_STATUSES`, `BOOT_INTERVAL`) and Heartbeat. |

## Modules of `cp-sim`

Layered from the wire up; each module uses only the ones above it.

| Module | Responsibility | Depends on the OCPP version? |
|---|---|---|
| `frame` | OCPP-J RPC frame: CALL, CALLRESULT, CALLERROR as a JSON array. | No, same in 1.6 and 2.0.1. |
| `ocpp16` | OCPP 1.6 payloads; the `Request` trait ties a payload to its action name and response type. | Yes. |
| `pending` | `PendingCalls`: outstanding CALLs by message ID, each waiting on a `oneshot` channel. | No. |
| `client` | One WebSocket connection: `Client::call` sends a typed request and waits for the typed answer; the reader task routes incoming frames. | Partly: sub-protocol `ocpp1.6`, `ocpp16::Request`. |
| `boot` | Maps a BootNotification response to the next step (Accepted, Pending, Rejected). | Yes, 1.6 section 4.2. |
| `backoff` | Wait before a reconnect, OCPP-J 2.0.1 part 4 section 5.4. | No, a transport rule. |
| `session` | Stays connected forever: connect, boot until Accepted, Heartbeat, reconnect. Owns the state that survives a connection (`Station`). | Yes, through `boot` and `ocpp16`. |
| `main` | Reads the CSMS URL, rejects a malformed one, fixes the settings, runs `session::run`. | Boot payload is 1.6. |
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

Connection life cycle (`session`): connect → BootNotification until
Accepted → Heartbeat every `interval` → on loss, wait (back-off or the
Rejected interval) and connect again. When the reader task ends, every
waiting and later CALL on that client fails with `Disconnected`, which is
how `session` learns that the connection is gone.

## Invariants

Each one is enforced by the tests named here (`module::tests::name`).

| Invariant | Tests |
|---|---|
| At most one outgoing CALL waits for an answer at a time (OCPP-J). | `client::only_one_call_in_flight` |
| A response is matched by message ID only; an unknown, late or repeated ID is dropped, and a CALL never completes a pending entry. | `client::call_times_out_and_late_response_is_ignored`, `client::ignores_malformed_and_unrelated_frames`, `pending::unknown_id_returns_frame_unchanged`, `pending::second_response_with_same_id_is_rejected`, `pending::call_frame_is_not_a_response` |
| Every CALL from the CSMS gets an answer (CALLERROR `NotImplemented` until handlers exist). | `client::answers_server_call_with_not_implemented` |
| Once the connection is gone, waiting and later CALLs fail at once with `Disconnected` instead of running into their timeout. | `client::calls_fail_at_once_when_connection_closes`, `client::close_ends_the_connection_and_fails_calls`, `pending::close_fails_waiting_and_later_calls` |
| `interval = 0` from the CSMS never reaches `tokio::time::interval` (it panics); a default is used. | `boot::accepted_with_zero_interval_uses_default_heartbeat`, `boot::pending_with_zero_interval_uses_default_retry`, `boot::rejected_with_zero_interval_uses_default_retry` |
| Pending boots again on the same connection; Rejected closes it and boots again on a new one. | `session::pending_boots_again_on_same_connection_after_interval`, `session::rejected_reconnects_after_interval_and_boots_again` |
| After Accepted, a reconnect does not send BootNotification again. | `session::after_accepted_reconnects_without_boot`, `session::connection_lost_while_pending_boots_again` |
| A failed CALL (timeout, CALLERROR) on a live connection does not end the connection or the program. | `session::failed_boot_is_retried_on_same_connection`, `session::heartbeat_call_error_keeps_connection`, `session::heartbeat_timeout_keeps_connection` |
| Back-off grows while connecting fails and resets only after a successful OCPP exchange, not after a bare handshake. | `session::backoff_grows_while_connect_fails`, `session::handshake_alone_does_not_reset_backoff`, `session::successful_exchange_resets_backoff`, `backoff::doubles_up_to_repeat_times_then_stays` |
| Back-off arithmetic saturates instead of panicking on huge settings. | `backoff::huge_settings_saturate_instead_of_panicking` |
| The client offers sub-protocol `ocpp1.6` in the handshake. | None yet: the fake CSMS answers `ocpp1.6` without looking at the request. Backlog. |
| A malformed or non-`ws(s)://` URL stops the program at startup instead of being retried forever. | None yet: the check lives in `main`. Backlog. |

## Extension points for OCPP 2.0.1

1.6 is wired in at three places today: the `ocpp16` module, the
sub-protocol header in `Client::connect`, and the `ocpp16::Request` bound
of `Client::call`. `frame`, `pending` and `backoff` are already
version-independent.

The plan (roadmap, "Larger blocks"): extract the version-independent
parts and the `Request` trait into an `ocpp-proto` library crate (an
earlier attempt is kept in `docs/archive/`), add a `v201` message module
beside `v16`, let the version pick the sub-protocol, and give `session`
a per-version boot and status logic. Behaviour that differs between
versions (for example `boot::next_step`, which follows 1.6 section 4.2)
belongs to the version module, not to `session`.
