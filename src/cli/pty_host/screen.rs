//! What the host reads from a child's screen and the bytes it sends for a key, in tmux's terms.

/// The bytes a terminal sends for one tmux key name: `swarm answer` sends a digit, Up, Down, or
/// Enter, interrupt sends Escape, the app sends Up and C-u, and `clear-box` sends M-Up, C-e, C-u,
/// and BSpace. A name that is no key, such as `10`, is sent as text, as `tmux send-keys` sends it.
pub(super) fn key_bytes(key: &str, application_cursor: bool) -> Vec<u8> {
    let arrow = |name: &str| match name {
        "Up" => Some('A'),
        "Down" => Some('B'),
        _ => None,
    };
    match key {
        "Enter" => b"\r".to_vec(),
        "Escape" => b"\x1b".to_vec(),
        "BSpace" => b"\x7f".to_vec(),
        _ if let Some(letter) = arrow(key) => match application_cursor {
            true => format!("\x1bO{letter}").into_bytes(),
            false => format!("\x1b[{letter}").into_bytes(),
        },
        // xterm's arrow with Alt held, which tmux sends for M-Up.
        _ if let Some(letter) = key.strip_prefix("M-").and_then(arrow) => {
            format!("\x1b[1;3{letter}").into_bytes()
        }
        _ if let Some(&[letter @ b'a'..=b'z']) = key.strip_prefix("C-").map(str::as_bytes) => {
            vec![letter & 0x1f]
        }
        _ => key.as_bytes().to_vec(),
    }
}

/// The visible rows, as `tmux capture-pane -p` prints them.
pub(super) fn screen(parser: &vt100::Parser) -> String {
    let screen = parser.screen();
    let (_, cols) = screen.size();
    let rows: Vec<String> = screen.rows(0, cols).collect();
    rows.join("\n")
}

/// The history and the visible rows, with wrapped rows joined, as `capture-pane -p -J -S -`.
pub(super) fn capture(parser: &mut vt100::Parser) -> String {
    let screen = parser.screen_mut();
    let (rows, cols) = screen.size();
    screen.set_scrollback(usize::MAX);
    let history = screen.scrollback();
    let mut text = String::new();
    let mut push = |screen: &vt100::Screen, row: u16| {
        text.push_str(&screen.rows(0, cols).nth(row.into()).unwrap_or_default());
        if !screen.row_wrapped(row) {
            text.push('\n');
        }
    };
    // vt100 shows history only through the scrolled view, so read its top row at each offset.
    for offset in (1..=history).rev() {
        screen.set_scrollback(offset);
        push(screen, 0);
    }
    screen.set_scrollback(0);
    for row in 0..rows {
        push(screen, row);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_keys_by_cursor_mode() {
        assert_eq!(key_bytes("Up", false), b"\x1b[A");
        assert_eq!(key_bytes("Up", true), b"\x1bOA");
        assert_eq!(key_bytes("2", false), b"2");
        assert_eq!(key_bytes("10", false), b"10");
        assert_eq!(key_bytes("C-u", false), b"\x15");
        assert_eq!(key_bytes("C-e", false), b"\x05");
        assert_eq!(key_bytes("M-Up", false), b"\x1b[1;3A");
        assert_eq!(key_bytes("BSpace", false), b"\x7f");
    }

    #[test]
    fn capture_joins_wrapped_rows_after_the_history() {
        let mut parser = vt100::Parser::new(2, 4, 10);
        parser.process(b"one\r\ntwo\r\nabcdefg\r\nend");
        assert_eq!(capture(&mut parser), "one\ntwo\nabcdefg\nend\n");
        assert_eq!(screen(&parser), "efg\nend");
        assert_eq!(parser.screen().scrollback(), 0);
    }
}
