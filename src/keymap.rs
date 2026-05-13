use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone)]
pub enum Action {
    Quit,
    NavUp,
    NavDown,
    Restart,
    Stop,
    StopAll,
    ToggleFocus,
    StartFilter,
    ClearLogs,
    FilterChar(char),
    FilterBackspace,
    FilterSubmit,
    FilterCancel,
    ToggleTap,
    ToggleTapDetail,
    ToggleGraph,
    ToggleHelp,
    ScrollLogsUp,
    ScrollLogsDown,
    ScrollLogsTop,
    ScrollLogsBottom,
    ShareNow,
    None,
}

pub fn map(ev: KeyEvent, filter_mode: bool) -> Action {
    if filter_mode {
        return match ev.code {
            KeyCode::Esc => Action::FilterCancel,
            KeyCode::Enter => Action::FilterSubmit,
            KeyCode::Backspace => Action::FilterBackspace,
            KeyCode::Char(c) => Action::FilterChar(c),
            _ => Action::None,
        };
    }
    match (ev.code, ev.modifiers) {
        (KeyCode::Char('q'), _) => Action::Quit,
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
        (KeyCode::Char('c'), _) => Action::ClearLogs,
        (KeyCode::Char('j'), _) | (KeyCode::Down, _) => Action::NavDown,
        (KeyCode::Char('k'), _) | (KeyCode::Up, _) => Action::NavUp,
        (KeyCode::Char('r'), _) => Action::Restart,
        (KeyCode::Char('S'), _) => Action::StopAll,
        (KeyCode::Char('s'), _) => Action::ShareNow,
        (KeyCode::Enter, _) => Action::ToggleFocus,
        (KeyCode::Char('/'), _) => Action::StartFilter,
        (KeyCode::Char('t'), _) => Action::ToggleTap,
        (KeyCode::Char('T'), _) => Action::ToggleTapDetail,
        (KeyCode::Char('g'), KeyModifiers::CONTROL) => Action::ScrollLogsTop,
        (KeyCode::Char('g'), _) => Action::ToggleGraph,
        (KeyCode::Char('?'), _) => Action::ToggleHelp,
        (KeyCode::Char('u'), _) | (KeyCode::PageUp, _) => Action::ScrollLogsUp,
        (KeyCode::Char('d'), _) | (KeyCode::PageDown, _) => Action::ScrollLogsDown,
        (KeyCode::Char('G'), _) => Action::ScrollLogsBottom,
        (KeyCode::Home, _) => Action::ScrollLogsTop,
        (KeyCode::End, _) => Action::ScrollLogsBottom,
        (KeyCode::Char('x'), _) => Action::Stop,
        (KeyCode::Esc, _) => Action::ToggleHelp,
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    #[test]
    fn basic_maps() {
        assert!(matches!(map(k('q'), false), Action::Quit));
        assert!(matches!(map(k('j'), false), Action::NavDown));
        assert!(matches!(map(k('k'), false), Action::NavUp));
        assert!(matches!(map(k('r'), false), Action::Restart));
        assert!(matches!(map(k('x'), false), Action::Stop));
        assert!(matches!(map(k('S'), false), Action::StopAll));
        assert!(matches!(map(k('s'), false), Action::ShareNow));
        assert!(matches!(map(k('/'), false), Action::StartFilter));
    }

    #[test]
    fn ctrl_c_quits() {
        let ev = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(matches!(map(ev, false), Action::Quit));
    }

    #[test]
    fn filter_mode_takes_chars() {
        assert!(matches!(map(k('x'), true), Action::FilterChar('x')));
        let ev = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert!(matches!(map(ev, true), Action::FilterCancel));
    }

    #[test]
    fn tap_keys() {
        assert!(matches!(map(k('t'), false), Action::ToggleTap));
        assert!(matches!(map(k('T'), false), Action::ToggleTapDetail));
    }

    #[test]
    fn graph_and_help_keys() {
        assert!(matches!(map(k('g'), false), Action::ToggleGraph));
        assert!(matches!(map(k('?'), false), Action::ToggleHelp));
    }

    #[test]
    fn esc_outside_filter_closes_overlays() {
        let ev = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert!(matches!(map(ev, false), Action::ToggleHelp));
    }

    #[test]
    fn scroll_keys_map() {
        assert!(matches!(map(k('u'), false), Action::ScrollLogsUp));
        assert!(matches!(map(k('d'), false), Action::ScrollLogsDown));
        assert!(matches!(map(k('G'), false), Action::ScrollLogsBottom));
        let home = KeyEvent::new(KeyCode::Home, KeyModifiers::NONE);
        assert!(matches!(map(home, false), Action::ScrollLogsTop));
        let ctrl_g = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL);
        assert!(matches!(map(ctrl_g, false), Action::ScrollLogsTop));
        let pgup = KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE);
        assert!(matches!(map(pgup, false), Action::ScrollLogsUp));
    }

    #[test]
    fn share_key_maps() {
        assert!(matches!(map(k('s'), false), Action::ShareNow));
    }
}
