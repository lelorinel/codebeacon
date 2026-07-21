//! Nav / Attach key dispatch (hjkl + arrows; attach only intercepts Ctrl+]).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    Sidebar,
    Prompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavAction {
    SidebarUp,
    SidebarDown,
    Attach,
    NewPane,
    ClosePane,
    FocusPrompt,
    Quit,
    PromptSubmit,
    PromptChar(char),
    PromptBackspace,
    PromptCancel,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachAction {
    Detach,
    PassThrough,
}

/// Map a key in Nav mode. `focus` selects sidebar vs bottom prompt bar.
pub fn nav_action(key: KeyEvent, focus: FocusTarget) -> NavAction {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return NavAction::None;
    }
    match focus {
        FocusTarget::Prompt => match key.code {
            KeyCode::Esc => NavAction::PromptCancel,
            KeyCode::Enter => NavAction::PromptSubmit,
            KeyCode::Backspace => NavAction::PromptBackspace,
            KeyCode::Char(c) => NavAction::PromptChar(c),
            _ => NavAction::None,
        },
        FocusTarget::Sidebar => match key.code {
            KeyCode::Char('j') | KeyCode::Down => NavAction::SidebarDown,
            KeyCode::Char('k') | KeyCode::Up => NavAction::SidebarUp,
            KeyCode::Enter => NavAction::Attach,
            KeyCode::Char('n') => NavAction::NewPane,
            KeyCode::Char('x') => NavAction::ClosePane,
            KeyCode::Char('p') => NavAction::FocusPrompt,
            KeyCode::Char('Q') => NavAction::Quit,
            _ => NavAction::None,
        },
    }
}

/// Attach mode: only Ctrl+] detaches; everything else passes to the PTY.
pub fn attach_action(key: KeyEvent) -> AttachAction {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char(']') {
        return AttachAction::Detach;
    }
    AttachAction::PassThrough
}

/// Encode a key event as bytes for the PTY writer.
pub fn key_to_pty_bytes(key: KeyEvent) -> Vec<u8> {
    let mods = key.modifiers;
    match key.code {
        KeyCode::Char(c) if mods.contains(KeyModifiers::CONTROL) => {
            let lower = c.to_ascii_lowercase();
            if ('a'..='z').contains(&lower) {
                vec![(lower as u8) - b'a' + 1]
            } else if c == ']' {
                vec![0x1d] // Ctrl+]
            } else {
                Vec::new()
            }
        }
        KeyCode::Char(c) => c.to_string().into_bytes(),
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn hjkl_and_arrows_same_sidebar_nav() {
        assert_eq!(
            nav_action(key(KeyCode::Char('j')), FocusTarget::Sidebar),
            NavAction::SidebarDown
        );
        assert_eq!(
            nav_action(key(KeyCode::Down), FocusTarget::Sidebar),
            NavAction::SidebarDown
        );
        assert_eq!(
            nav_action(key(KeyCode::Char('k')), FocusTarget::Sidebar),
            NavAction::SidebarUp
        );
        assert_eq!(
            nav_action(key(KeyCode::Up), FocusTarget::Sidebar),
            NavAction::SidebarUp
        );
    }

    #[test]
    fn attach_only_ctrl_bracket_detaches() {
        assert_eq!(attach_action(ctrl(']')), AttachAction::Detach);
        assert_eq!(attach_action(key(KeyCode::Esc)), AttachAction::PassThrough);
        assert_eq!(attach_action(ctrl('c')), AttachAction::PassThrough);
        assert_eq!(attach_action(key(KeyCode::Char('j'))), AttachAction::PassThrough);
    }

    #[test]
    fn quit_is_capital_q_only() {
        assert_eq!(
            nav_action(key(KeyCode::Char('Q')), FocusTarget::Sidebar),
            NavAction::Quit
        );
        assert_eq!(
            nav_action(key(KeyCode::Char('q')), FocusTarget::Sidebar),
            NavAction::None
        );
    }

    #[test]
    fn attach_action_does_not_block_enter() {
        // Ensemble gating is in the TUI loop; keys still emit Attach.
        assert_eq!(
            nav_action(key(KeyCode::Enter), FocusTarget::Sidebar),
            NavAction::Attach
        );
    }
}
