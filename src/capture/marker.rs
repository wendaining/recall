//! Streaming filter that strips recall's private in-band end marker from the
//! terminal byte stream.
//!
//! The shell writes `ESC ] 9999 ; recall-end BEL` right before the next prompt.
//! Because it travels in-band, the proxy can stop capturing at the exact byte
//! boundary, so the prompt that follows is never attributed to the command.
//! Terminals ignore unknown OSC sequences, and the marker is only emitted while
//! the proxy is active.

/// The private end-of-output marker.
pub const END_MARKER: &[u8] = b"\x1b]9999;recall-end\x07";

/// Result of feeding a chunk through the filter.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Filtered {
    /// Bytes before the first marker: belong to the command output.
    pub before: Vec<u8>,
    /// Number of complete markers stripped from this chunk.
    pub ends: usize,
    /// Bytes after the first marker: already past the command output.
    pub after: Vec<u8>,
}

#[derive(Default)]
pub struct MarkerFilter {
    leftover: Vec<u8>,
}

impl MarkerFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, data: &[u8]) -> Filtered {
        let mut buf = std::mem::take(&mut self.leftover);
        buf.extend_from_slice(data);

        let Some(pos) = find_subslice(&buf, END_MARKER) else {
            let hold = partial_prefix_len(&buf, END_MARKER);
            let keep = buf.len() - hold;
            let before = buf[..keep].to_vec();
            self.leftover = buf[keep..].to_vec();
            return Filtered {
                before,
                ends: 0,
                after: Vec::new(),
            };
        };

        let before = buf[..pos].to_vec();
        let mut after = Vec::new();
        let mut ends = 1;
        let mut rest = &buf[pos + END_MARKER.len()..];
        loop {
            match find_subslice(rest, END_MARKER) {
                Some(p) => {
                    after.extend_from_slice(&rest[..p]);
                    ends += 1;
                    rest = &rest[p + END_MARKER.len()..];
                }
                None => {
                    let hold = partial_prefix_len(rest, END_MARKER);
                    let keep = rest.len() - hold;
                    after.extend_from_slice(&rest[..keep]);
                    self.leftover = rest[keep..].to_vec();
                    break;
                }
            }
        }

        Filtered {
            before,
            ends,
            after,
        }
    }

    /// Flush any held partial-marker bytes (call once at end of stream).
    pub fn flush(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.leftover)
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Length of the longest suffix of `tail` that is a proper prefix of `marker`.
fn partial_prefix_len(tail: &[u8], marker: &[u8]) -> usize {
    if marker.is_empty() {
        return 0;
    }
    let max = tail.len().min(marker.len() - 1);
    for k in (1..=max).rev() {
        if tail[tail.len() - k..] == marker[..k] {
            return k;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_marker_and_splits() {
        let mut filter = MarkerFilter::new();
        let mut input = b"hello\n".to_vec();
        input.extend_from_slice(END_MARKER);
        input.extend_from_slice(b"prompt$ ");
        let out = filter.feed(&input);
        assert_eq!(out.before, b"hello\n");
        assert_eq!(out.ends, 1);
        assert_eq!(out.after, b"prompt$ ");
    }

    #[test]
    fn handles_marker_split_across_chunks() {
        let mut filter = MarkerFilter::new();
        let split = 5;
        let first = filter.feed(&END_MARKER[..split]);
        assert!(first.before.is_empty());
        assert_eq!(first.ends, 0);
        let second = filter.feed(&END_MARKER[split..]);
        assert_eq!(second.ends, 1);
    }

    #[test]
    fn passes_through_normal_escapes() {
        let mut filter = MarkerFilter::new();
        let out = filter.feed(b"\x1b[31mred\x1b[0m");
        assert_eq!(out.before, b"\x1b[31mred\x1b[0m");
        assert_eq!(out.ends, 0);
        assert!(out.after.is_empty());
    }

    #[test]
    fn holds_partial_escape_prefix() {
        let mut filter = MarkerFilter::new();
        let out = filter.feed(b"abc\x1b]99");
        assert_eq!(out.before, b"abc");
        assert_eq!(filter.flush(), b"\x1b]99");
    }
}
