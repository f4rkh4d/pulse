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
        (KeyCode::Char('s'), _) => Action::Stop,
        (KeyCode::Enter, _) => Action::ToggleFocus,
        (KeyCode::Char('/'), _) => Action::StartFilter,
        (KeyCode::Char('t'), _) => Action::ToggleTap,
        (KeyCode::Char('T'), _) => Action::ToggleTapDetail,
        (KeyCode::Char('g'), _) => Action::ToggleGraph,
        (KeyCode::Char('?'), _) => Action::ToggleHelp,
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
        assert!(matches!(map(k('s'), false), Action::Stop));
        assert!(matches!(map(k('S'), false), Action::StopAll));
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
}
