# Archive

Reference material from before the migration. **Not code to apply**: the
patch is against the old standalone `cp-sim` layout and conflicts with the
code in this repository.

## `ocpp-proto-stash.diff`

An abandoned attempt (October 2026) to split the old `cp-sim` crate into a
cargo workspace with two crates:

- `crates/ocpp-proto`: protocol layer without networking. `Frame` with
  constructors (`Frame::call`, `result`, `error`, `parse`), a `FrameError`
  type based on `thiserror`, the `Request` trait and the OCPP 1.6
  BootNotification / Heartbeat payloads in `v16/core.rs`.
- `crates/cp-sim`: the client binary using it.

It was kept as a git stash and never committed. The client code in
`crates/cp-sim` was developed separately afterwards, so the two diverge,
for example:

| | stash | current `crates/cp-sim` |
|---|---|---|
| `interval` type | `u32`, clamped to at least 1 | `u64`, no guard against 0 |
| `Request` bound | `Serialize + DeserializeOwned` | `Serialize` |
| Error type | `FrameError` (`thiserror`) | `serde_json::Error` |
| Response matching | not yet | by message ID (`PendingCalls`) |

Some comments in the patch are in Russian.

Use it as input for the roadmap task "extract an `ocpp-proto` library
crate". It contained no untracked files; the new crates were staged when
the stash was created, so the patch is complete.
