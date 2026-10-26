//! What the host reads from a child's screen and the bytes it sends for a key, in tmux's terms.

/// The bytes a terminal sends for one tmux key name: `swarm answer` sends a digit, Up, Down, or
/// Enter, and the app sends Up and C-u.
pub(super) fn key_bytes(key: &str, application_cursor: bool) -> Result<Vec<u8>, String> {
    let arrow = |letter: char| match application_cursor {
        true => format!("\x1bO{letter}"),
        false => format!("\x1b[{letter}"),
    };
    Ok(match key {
        "Enter" => "\r".into(),
        "Escape" => "\x1b".into(),
        "Tab" => "\t".into(),
        "BTab" => "\x1b[Z".into(),
        "BSpace" => "\x7f".into(),
        "Space" => " ".into(),
        "Up" => arrow('A'),
        "Down" => arrow('B'),
        "Right" => arrow('C'),
        "Left" => arrow('D'),
        "C-c" => "\x03".into(),
        "C-u" => "\x15".into(),
        _ if key.chars().count() == 1 => key.into(),
        _ => return Err(format!("swarm pty: unknown key {key}")),
    }
    .into_bytes())
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
        assert_eq!(key_bytes("Up", false).unwrap(), b"\x1b[A");
        assert_eq!(key_bytes("Up", true).unwrap(), b"\x1bOA");
        assert_eq!(key_bytes("2", false).unwrap(), b"2");
        assert_eq!(key_bytes("C-u", false).unwrap(), b"\x15");
        assert!(key_bytes("F13", false).is_err());
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
