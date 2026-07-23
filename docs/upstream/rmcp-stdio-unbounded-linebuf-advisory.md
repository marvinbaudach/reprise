# rmcp Security Advisory — eingereicht

**Status:** Eingereicht als öffentliches Issue am 2026-07-23:
https://github.com/modelcontextprotocol/rust-sdk/issues/1030

Kanal-Historie: Private Vulnerability Reporting ist im Repo aktiviert, aber der
DoS wurde bewusst als öffentliches Issue gemeldet (Entscheidung 2026-07-23).
Der frühere REST-API-Versuch über `.../security-advisories/reports` lieferte am
2026-07-22 HTTP 500 (GitHub-seitig). Text unten = eingereichter Inhalt.

**Title:** Unbounded line buffer in AsyncRwTransport::receive (stdio transport) — memory-exhaustion denial-of-service

**Severity:** Medium (DoS, untrusted-input, no data exposure)

---

### Summary

`AsyncRwTransport::receive` — the read path used by the stdio server transport (`rmcp::transport::io::stdio()`) and by the `TokioChildProcess` client transport — buffers an incoming line with `BufReader::read_until(b'\n', &mut self.line_buf)` and never checks a maximum length. A peer that sends a single line without a `\n` (or an extremely long line) makes the process grow `line_buf: Vec<u8>` without bound, exhausting memory. `JsonRpcMessageCodec` already has a `max_length` field with correct enforcement logic, but that logic lives in `Decoder::decode`, which `AsyncRwTransport` never calls — the transport's read side does not go through the codec's decoder at all. In the crate itself, `Decoder::decode` is only exercised by the crate's own test helper (`from_async_read`, via `FramedRead`, in `#[cfg(test)] mod test`).

### Affected versions

Confirmed in `rmcp` 2.2.0 (current latest release) and on `main` (files are byte-identical as of this report). The specific `BufReader` + `line_buf: Vec<u8>` + uncapped `read_until` pattern is present at least back to 1.8.0. Earlier releases (checked at 1.0.0) route `receive()` through `FramedRead` + `Decoder::decode` instead, but `AsyncRwTransport::new` always constructs `JsonRpcMessageCodec::default()` (`max_length: usize::MAX`) with no way for a caller to lower it — so those versions are equally unbounded in practice via a different internal path. Not bisected earlier than 1.0.0.

### Code reference

`crates/rmcp/src/transport/async_rw.rs`, `AsyncRwTransport::receive`:

```rust
async fn receive(&mut self) -> Option<RxJsonRpcMessage<Role>> {
    loop {
        match self.read.read_until(b'\n', &mut self.line_buf).await {
            Ok(0) => return None,
            Ok(_) => {}
            Err(e) => {
                tracing::error!("Error reading from stream: {}", e);
                return None;
            }
        }
        // ... line_buf is only cleared after a full '\n'-terminated line
        // has been parsed; nothing bounds it before that.
```

`line_buf` is declared as a plain `Vec<u8>` with no capacity ceiling. Meanwhile `JsonRpcMessageCodec::max_length` is fully implemented and correct inside `Decoder::decode`, but is unreachable from this transport.

### Impact

Any stdio MCP server built with this SDK (`.serve(rmcp::transport::io::stdio())`), and any client using `TokioChildProcess`, accepts an unbounded amount of memory from a single unterminated or oversized input line from an untrusted peer — a straightforward denial-of-service.

### Minimal reproduction sketch

```rust
// Server: any rmcp stdio server started the standard way, e.g.
// server.serve(rmcp::transport::io::stdio()).await?;

// Attacker / harness, writing to the server's stdin:
let chunk = vec![b'A'; 1 << 20]; // 1 MiB, contains no b'\n'
loop {
    stdin.write_all(&chunk).await?;
}
// Observe the server process's RSS grow without bound (e.g. `ps -o rss=`
// or /proc/<pid>/status) and never shrink, since line_buf is only cleared
// once a full '\n'-terminated line has been parsed.
```

### Suggested fix

- Give `AsyncRwTransport::new`/`new_client`/`new_server` a way to configure a maximum line length, and check `self.line_buf.len()` against it inside the `read_until` loop in `receive`, erroring out (and clearing the buffer) instead of continuing to append past the limit.
- Alternatively, route reads back through `JsonRpcMessageCodec`'s existing, correct `max_length` handling in `Decoder::decode`, while preserving the cancellation-safety behavior added in #947.
- A conservative built-in default (e.g. a few MiB) would close the gap for existing callers with no API change; exposing it as configurable would help callers who legitimately need larger single-line payloads.

### Workaround

Until fixed, downstream consumers can wrap the reader passed to `stdio()`/`serve()` in their own size-capped `AsyncRead` that errors once a bounded number of bytes has been read without a newline.

---
Found while integrating the SDK into a desktop application. Happy to provide more detail, a runnable repro, or test against a candidate patch.
