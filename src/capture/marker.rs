//! Streaming parser for recall's private in-band control markers.
//!
//! The shell writes newline-free JSON wrapped in a private OSC sequence:
//!
//! ```text
//! ESC ] 9999 ; {"type":"start",...} BEL
//! ESC ] 9999 ; {"type":"end",...}   BEL
//! ```
//!
//! Terminals ignore unknown OSC numbers, and the proxy strips these before
//! forwarding, so they never reach the screen. Because the markers travel
//! in-band, command boundaries are exact: everything between a `start` and its
//! `end` belongs to that command.
//!
//! The parser is optimised for the common case: a chunk with no marker is
//! returned borrowed, with no allocation or copy.

use std::borrow::Cow;

use memchr::memmem;

use crate::capture::protocol::Request;

/// The private OSC prefix used for recall's markers.
const PREFIX: &[u8] = b"\x1b]9999;";
/// Safety cap for a marker that never terminates; beyond this it is forwarded.
const MAX_PENDING: usize = 1 << 20;

/// An ordered operation produced while parsing a chunk.
pub enum Op {
    /// Bytes to forward (and capture, depending on state).
    Bytes(Vec<u8>),
    /// A control marker decoded from the stream.
    Event(Request),
}

/// Result of feeding a chunk through the filter.
pub enum Feed<'a> {
    /// No marker present: forward the input slice unchanged (zero copy).
    Plain(&'a [u8]),
    /// Markers present: process the ordered operations.
    Ops(Vec<Op>),
}

#[derive(Default)]
pub struct MarkerFilter {
    /// Bytes of a marker that was split across a chunk boundary.
    pending: Vec<u8>,
}

impl MarkerFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed<'a>(&mut self, data: &'a [u8]) -> Feed<'a> {
        if self.pending.is_empty() && memmem::find(data, PREFIX).is_none() {
            let hold = partial_prefix_len(data, PREFIX);
            if hold == 0 {
                return Feed::Plain(data);
            }
            let split = data.len() - hold;
            let mut ops = Vec::with_capacity(1);
            if split > 0 {
                ops.push(Op::Bytes(data[..split].to_vec()));
            }
            self.pending.extend_from_slice(&data[split..]);
            return Feed::Ops(ops);
        }

        let buf: Cow<'_, [u8]> = if self.pending.is_empty() {
            Cow::Borrowed(data)
        } else {
            let mut owned = std::mem::take(&mut self.pending);
            owned.extend_from_slice(data);
            Cow::Owned(owned)
        };

        let mut ops: Vec<Op> = Vec::new();
        let buf = buf.as_ref();
        let mut last = 0usize;
        let mut i = 0usize;
        while i < buf.len() {
            if buf[i] != 0x1b {
                i += 1;
                continue;
            }
            let rest = &buf[i..];
            if let Some(payload) = rest.strip_prefix(PREFIX) {
                if let Some((payload_len, term_len)) = find_terminator(payload) {
                    if last < i {
                        ops.push(Op::Bytes(buf[last..i].to_vec()));
                    }
                    if let Ok(event) = serde_json::from_slice::<Request>(&payload[..payload_len]) {
                        ops.push(Op::Event(event));
                    }
                    i += PREFIX.len() + payload_len + term_len;
                    last = i;
                    continue;
                }
                // Incomplete marker: hold from here until the next chunk.
                if last < i {
                    ops.push(Op::Bytes(buf[last..i].to_vec()));
                }
                if rest.len() <= MAX_PENDING {
                    self.pending.extend_from_slice(rest);
                } else {
                    ops.push(Op::Bytes(rest.to_vec()));
                }
                return Feed::Ops(ops);
            }
            let hold = partial_prefix_len(rest, PREFIX);
            if hold > 0 {
                if last < i {
                    ops.push(Op::Bytes(buf[last..i].to_vec()));
                }
                self.pending.extend_from_slice(&rest[..hold]);
                return Feed::Ops(ops);
            }
            i += 1;
        }

        if last < buf.len() {
            ops.push(Op::Bytes(buf[last..].to_vec()));
        }
        Feed::Ops(ops)
    }

    /// Flush a held partial marker at end of stream.
    pub fn flush(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pending)
    }
}

/// Find the end of an OSC payload (BEL or ST). Returns `(payload_len, term_len)`.
fn find_terminator(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            0x07 => return Some((i, 1)),
            0x1b => {
                if i + 1 < bytes.len() {
                    if bytes[i + 1] == b'\\' {
                        return Some((i, 2));
                    }
                    i += 1;
                } else {
                    return None;
                }
            }
            _ => i += 1,
        }
    }
    None
}

/// Length of the longest suffix of `haystack` that is a proper prefix of `prefix`.
fn partial_prefix_len(haystack: &[u8], prefix: &[u8]) -> usize {
    if prefix.is_empty() {
        return 0;
    }
    let max = haystack.len().min(prefix.len() - 1);
    for k in (1..=max).rev() {
        if haystack[haystack.len() - k..] == prefix[..k] {
            return k;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(feed: Feed<'_>) -> (Vec<u8>, Vec<Request>) {
        let mut bytes = Vec::new();
        let mut events = Vec::new();
        match feed {
            Feed::Plain(chunk) => bytes.extend_from_slice(chunk),
            Feed::Ops(ops) => {
                for op in ops {
                    match op {
                        Op::Bytes(b) => bytes.extend_from_slice(&b),
                        Op::Event(e) => events.push(e),
                    }
                }
            }
        }
        (bytes, events)
    }

    fn marker(json: &str) -> Vec<u8> {
        let mut v = PREFIX.to_vec();
        v.extend_from_slice(json.as_bytes());
        v.push(0x07);
        v
    }

    #[test]
    fn plain_chunk_is_borrowed() {
        let mut filter = MarkerFilter::new();
        assert!(matches!(filter.feed(b"hello world"), Feed::Plain(_)));
    }

    #[test]
    fn parses_start_and_strips_marker() {
        let mut filter = MarkerFilter::new();
        let mut input = b"prompt$ ".to_vec();
        input.extend(marker(
            r#"{"type":"start","id":"a","command":"ls","cwd":"/tmp","started_at":1}"#,
        ));
        input.extend_from_slice(b"file1\nfile2\n");
        let (bytes, events) = events(filter.feed(&input));
        assert_eq!(bytes, b"prompt$ file1\nfile2\n");
        assert_eq!(events.len(), 1);
        match &events[0] {
            Request::Start { id, command, .. } => {
                assert_eq!(id, "a");
                assert_eq!(command, "ls");
            }
            _ => panic!("expected start"),
        }
    }

    #[test]
    fn parses_end_marker() {
        let mut filter = MarkerFilter::new();
        let mut input = b"out\n".to_vec();
        input.extend(marker(
            r#"{"type":"end","id":"a","exit":2,"duration_ns":42}"#,
        ));
        let (bytes, events) = events(filter.feed(&input));
        assert_eq!(bytes, b"out\n");
        assert_eq!(events.len(), 1);
        match &events[0] {
            Request::End {
                id,
                exit,
                duration_ns,
            } => {
                assert_eq!(id, "a");
                assert_eq!(*exit, Some(2));
                assert_eq!(*duration_ns, Some(42));
            }
            _ => panic!("expected end"),
        }
    }

    #[test]
    fn handles_marker_split_across_chunks() {
        let mut filter = MarkerFilter::new();
        let full = marker(r#"{"type":"end","id":"x","exit":0}"#);
        let mid = full.len() / 2;
        let (bytes1, ev1) = events(filter.feed(&full[..mid]));
        assert!(bytes1.is_empty());
        assert!(ev1.is_empty());
        let (bytes2, ev2) = events(filter.feed(&full[mid..]));
        assert!(bytes2.is_empty());
        assert_eq!(ev2.len(), 1);
    }

    #[test]
    fn passes_through_color_escapes() {
        let mut filter = MarkerFilter::new();
        let (bytes, events) = events(filter.feed(b"\x1b[31mred\x1b[0m"));
        assert_eq!(bytes, b"\x1b[31mred\x1b[0m");
        assert!(events.is_empty());
    }

    #[test]
    fn holds_partial_prefix_at_tail() {
        let mut filter = MarkerFilter::new();
        let (bytes, ev1) = events(filter.feed(b"abc\x1b]99"));
        assert_eq!(bytes, b"abc");
        assert!(ev1.is_empty());
        let (bytes2, ev2) = events(filter.feed(b"99;{\"type\":\"end\",\"id\":\"z\"}\x07"));
        assert!(bytes2.is_empty());
        assert_eq!(ev2.len(), 1);
    }

    #[test]
    fn json_with_semicolons_and_escapes() {
        let mut filter = MarkerFilter::new();
        let input = marker(r#"{"type":"start","id":"a","command":"echo \"a;b\"","started_at":1}"#);
        let (_bytes, events) = events(filter.feed(&input));
        match &events[0] {
            Request::Start { command, .. } => assert_eq!(command, "echo \"a;b\""),
            _ => panic!("expected start"),
        }
    }
}
