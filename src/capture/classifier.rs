use crate::model::BlockKind;

/// Inputs for classifying a captured command output.
pub struct ClassifyInput<'a> {
    pub raw: &'a [u8],
    pub interactive: bool,
    pub max_output_bytes: usize,
    pub strip_ansi: bool,
    pub mark_interactive: bool,
}

/// Result of classification.
pub struct Classified {
    pub kind: BlockKind,
    pub output: Option<Vec<u8>>,
    pub truncated: bool,
}

/// Detect whether the captured bytes entered the alternate screen, which
/// indicates a full-screen interactive program.
pub fn detect_interactive(raw: &[u8]) -> bool {
    const MARKERS: [&[u8]; 2] = [b"\x1b[?1049h", b"\x1b[?47h"];
    MARKERS
        .iter()
        .any(|marker| raw.windows(marker.len()).any(|w| w == *marker))
}

/// Classify captured output and produce the storable plain-text form.
pub fn classify(input: ClassifyInput<'_>) -> Classified {
    let ClassifyInput {
        raw,
        interactive,
        max_output_bytes,
        strip_ansi,
        mark_interactive,
    } = input;

    if mark_interactive && interactive {
        return Classified {
            kind: BlockKind::Interactive,
            output: None,
            truncated: false,
        };
    }

    let text = if strip_ansi {
        crate::util::strip_ansi(raw)
    } else {
        raw.to_vec()
    };

    let text = trim_trailing(&text);

    if text.is_empty() {
        return Classified {
            kind: BlockKind::Empty,
            output: None,
            truncated: false,
        };
    }

    if is_binary(&text) {
        return Classified {
            kind: BlockKind::Binary,
            output: None,
            truncated: false,
        };
    }

    if text.len() > max_output_bytes {
        let mut out = text[..max_output_bytes].to_vec();
        out.extend_from_slice(b"\n[recall: output truncated]\n");
        return Classified {
            kind: BlockKind::Normal,
            output: Some(out),
            truncated: true,
        };
    }

    Classified {
        kind: BlockKind::Normal,
        output: Some(text),
        truncated: false,
    }
}

fn trim_trailing(data: &[u8]) -> Vec<u8> {
    let end = data
        .iter()
        .rposition(|&b| b != b'\n' && b != b'\r' && b != b' ' && b != b'\t')
        .map(|i| i + 1)
        .unwrap_or(0);
    data[..end].to_vec()
}

fn is_binary(data: &[u8]) -> bool {
    if data.is_empty() {
        return false;
    }
    let mut control = 0usize;
    for &b in data {
        if (b < 0x20 && b != b'\n' && b != b'\r' && b != b'\t') || b == 0x7f {
            control += 1;
        }
    }
    if control * 100 / data.len() > 30 {
        return true;
    }
    if std::str::from_utf8(data).is_ok() {
        return false;
    }
    let lossy = String::from_utf8_lossy(data);
    let replacement = lossy.matches('\u{FFFD}').count();
    replacement * 100 / lossy.chars().count().max(1) > 10
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BlockKind;

    fn classify_raw(raw: &[u8]) -> Classified {
        classify(ClassifyInput {
            raw,
            interactive: false,
            max_output_bytes: 1024,
            strip_ansi: true,
            mark_interactive: true,
        })
    }

    #[test]
    fn strips_ansi_and_keeps_text() {
        let out = classify_raw(b"\x1b[31mred\x1b[0m\n");
        assert_eq!(out.kind, BlockKind::Normal);
        assert_eq!(out.output.as_deref(), Some(b"red".as_slice()));
    }

    #[test]
    fn empty_output_is_empty() {
        let out = classify_raw(b"\n  \n");
        assert_eq!(out.kind, BlockKind::Empty);
        assert!(out.output.is_none());
    }

    #[test]
    fn detects_alt_screen() {
        assert!(detect_interactive(b"\x1b[?1049h"));
        let out = classify(ClassifyInput {
            raw: b"\x1b[?1049hstuff",
            interactive: true,
            max_output_bytes: 1024,
            strip_ansi: true,
            mark_interactive: true,
        });
        assert_eq!(out.kind, BlockKind::Interactive);
        assert!(out.output.is_none());
    }

    #[test]
    fn truncates_large_output() {
        let raw = vec![b'a'; 4096];
        let out = classify_raw(&raw);
        assert!(out.truncated);
        assert!(out.output.unwrap().len() < 4096);
    }
}
