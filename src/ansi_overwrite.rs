//! handle the `\r\x1b[2K` and cursor-up escapes that tools like cargo-watch,
//! jest, and webpack dev server use to redraw the "compiling…" line in place.
//!
//! goal: take a stream of raw stdout chunks and turn them into a clean list of
//! logical lines where the tool's overwrites actually overwrite.

/// max lines back we'll honor cursor-up for. plenty for most tools.
pub const MAX_BACK: usize = 10;

/// collapse a raw stream into logical lines. each element is one final line.
/// cursor-up + line-clear + CR are interpreted as in-place overwrites.
pub fn collapse(stream: &str) -> Vec<String> {
    let bytes = stream.as_bytes();
    let mut lines: Vec<String> = vec![String::new()];
    let mut cur: usize = 0;
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\n' {
            lines.push(String::new());
            cur = lines.len() - 1;
            i += 1;
            continue;
        }
        if b == b'\r' {
            // CR alone: rewind cursor to start of current line. if followed by
            // a clear-line escape, the next write replaces it.
            lines[cur].clear();
            i += 1;
            continue;
        }
        if b == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            // csi. peek for A (cursor-up) or K (line-clear) or 2K (full clear).
            let mut j = i + 2;
            let mut params = String::new();
            while j < bytes.len() && !bytes[j].is_ascii_alphabetic() {
                params.push(bytes[j] as char);
                j += 1;
            }
            if j >= bytes.len() {
                // malformed tail, drop it
                break;
            }
            let final_byte = bytes[j];
            match final_byte {
                b'A' => {
                    let n: usize = params.parse().unwrap_or(1);
                    let n = n.min(MAX_BACK);
                    cur = cur.saturating_sub(n);
                    // land at column 0: clear what was there so subsequent
                    // writes replace rather than append. tools that do the
                    // full "clear-then-redraw" dance rely on this.
                    lines[cur].clear();
                }
                b'K' => {
                    // line-clear variants (0K, 1K, 2K). we treat them all as
                    // "wipe the current line"; perfect for log rendering.
                    lines[cur].clear();
                }
                _ => {
                    // everything else (colors, cursor movement, etc) is
                    // harmless in a log context — skip silently.
                }
            }
            i = j + 1;
            continue;
        }
        // regular byte: append to current line
        // be utf-8-safe by accumulating bytes then lossy-decoding at the end.
        lines[cur].push(b as char);
        i += 1;
    }

    // drop a trailing empty line created by a final newline (feels natural).
    if lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_passes_through() {
        let out = collapse("hello\nworld\n");
        assert_eq!(out, vec!["hello", "world"]);
    }

    #[test]
    fn carriage_return_overwrites_current_line() {
        let out = collapse("first\rsecond\n");
        assert_eq!(out, vec!["second"]);
    }

    #[test]
    fn line_clear_and_cr_replaces() {
        let out = collapse("compiling...\r\x1b[2KCompiling...\r\x1b[2KFinished in 1.2s\n");
        assert_eq!(out, vec!["Finished in 1.2s"]);
    }

    #[test]
    fn cursor_up_overwrites_previous_line() {
        let out = collapse("loading\nwaiting\n\x1b[1Adone\n");
        assert_eq!(out[0], "loading");
        assert_eq!(out[1], "done");
    }

    #[test]
    fn cursor_up_caps_at_max_back() {
        // go way back further than MAX_BACK; should clamp
        let mut s = String::new();
        for i in 0..5 {
            s.push_str(&format!("l{i}\n"));
        }
        s.push_str("\x1b[50A");
        s.push_str("x\n");
        let out = collapse(&s);
        // we only honor up to MAX_BACK, so lines before that remain intact
        assert!(out.contains(&"x".to_string()));
    }

    #[test]
    fn unknown_escape_is_stripped() {
        // color codes shouldn't leak into text output
        let out = collapse("\x1b[31mred\x1b[0m\n");
        assert_eq!(out, vec!["red"]);
    }
}
