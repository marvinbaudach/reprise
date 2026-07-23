//! A size-capped `AsyncRead` guarding the stdio transport against a hostile
//! client.
//!
//! The pinned `rmcp` 2.2.0 stdio transport reads a frame with an unbounded
//! `read_until(b'\n', &mut line_buf)`: a client that sends a giant — or simply
//! newline-less — message makes `line_buf` grow without limit until the process
//! is OOM-killed. rmcp is a registry crate we do not patch, so we cap the input
//! *before* it reaches the transport by wrapping stdin in this adapter and
//! handing `serve` an `(AsyncRead, AsyncWrite)` pair.
//!
//! The adapter passes bytes straight through while counting how many have
//! arrived since the last newline. As soon as a single line's run exceeds
//! [`MAX_LINE_BYTES`] the reader is poisoned: the poll that detected the
//! overflow still returns its already-committed bytes (the `AsyncRead` contract
//! forbids returning `Err` from a poll that advanced the buffer), and the very
//! next poll — which reads nothing — returns a clean `io::Error`. The
//! transport's `read_until` surfaces that error, `receive` yields `None`, and
//! the server shuts down gracefully with memory bounded at roughly the cap plus
//! one buffer chunk rather than growing until it is killed.

use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{self, AsyncRead, ReadBuf};

/// Maximum bytes allowed in a single newline-delimited input line. 4 MiB is far
/// beyond any legitimate request — the largest tool call, a 500-track playlist
/// create, is a few tens of kilobytes — while still small enough that buffering
/// one capped line can never exhaust memory.
pub const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;

/// Wraps an `AsyncRead`, failing the read once a single line (bytes between
/// `\n` delimiters) exceeds `limit`. See the module docs for why this sits in
/// front of the rmcp stdio transport.
pub struct LineCappedReader<R> {
    inner: R,
    limit: usize,
    /// Bytes seen since the last newline. Reset to 0 at every `\n`.
    run: usize,
    /// Set once any line's run exceeded `limit`. Sticky: a later newline in the
    /// same chunk must not un-poison a line that already overflowed.
    overflowed: bool,
}

impl<R> LineCappedReader<R> {
    pub fn new(inner: R, limit: usize) -> Self {
        Self {
            inner,
            limit,
            run: 0,
            overflowed: false,
        }
    }

    fn overflow_error(&self) -> io::Error {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("input line exceeds {} bytes", self.limit),
        )
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for LineCappedReader<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // The struct is Unpin (all fields are), so projecting out `&mut Self` is
        // sound and lets us re-pin `inner` for the delegated read.
        let this = self.get_mut();
        // Once poisoned, fail on a poll that commits nothing — returning `Err`
        // from a poll that also advanced `buf` violates the `AsyncRead`
        // contract (and trips a debug assertion in tokio's `read_to_end`).
        if this.overflowed {
            return Poll::Ready(Err(this.overflow_error()));
        }
        let before = buf.filled().len();
        match Pin::new(&mut this.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                let fresh = &buf.filled()[before..];
                for &byte in fresh {
                    if byte == b'\n' {
                        this.run = 0;
                    } else {
                        this.run += 1;
                        if this.run > this.limit {
                            // Poison, but still return this chunk's bytes; the
                            // next (empty) poll surfaces the error.
                            this.overflowed = true;
                        }
                    }
                }
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    /// Reads the whole stream through a small cap; a line under the cap plus a
    /// newline is delivered intact, and the newline resets the run so a second
    /// line under the cap also passes.
    #[tokio::test]
    async fn passes_lines_under_the_cap_and_resets_at_newlines() {
        let input: &[u8] = b"aaaa\nbbbb\n";
        let mut reader = LineCappedReader::new(input, 4);
        let mut out = Vec::new();
        reader.read_to_end(&mut out).await.unwrap();
        assert_eq!(out, input);
    }

    /// A single run longer than the cap (no newline) fails the read rather than
    /// buffering unboundedly.
    #[tokio::test]
    async fn errors_once_a_single_line_exceeds_the_cap() {
        let input: &[u8] = b"aaaaaaaa"; // 8 bytes, no newline
        let mut reader = LineCappedReader::new(input, 4);
        let mut out = Vec::new();
        let error = reader.read_to_end(&mut out).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    /// The overflow is sticky: a newline later in the same chunk must not
    /// rescue a line that already exceeded the cap.
    #[tokio::test]
    async fn overflow_is_not_cleared_by_a_later_newline() {
        let input: &[u8] = b"aaaaaa\nbb\n"; // first line (6) already exceeds cap 4
        let mut reader = LineCappedReader::new(input, 4);
        let mut out = Vec::new();
        let error = reader.read_to_end(&mut out).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
