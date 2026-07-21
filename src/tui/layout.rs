//! Sidebar + focused PTY + prompt bar rendering.

use crate::tui::conductor::AgentRole;
use crate::tui::pane::{Pane, PaneStatus};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

pub struct LayoutFocus {
    pub sidebar_idx: usize,
    pub attached: bool,
    pub prompt_focused: bool,
    pub prompt: String,
    pub tick: u64,
    pub allow_new: bool,
    pub conductor_mode: bool,
    pub hint: String,
}

pub fn draw(f: &mut Frame, panes: &[Pane], focus: &LayoutFocus) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(f.area());

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(22), Constraint::Min(20)])
        .split(chunks[0]);

    draw_sidebar(f, body[0], panes, focus);
    draw_pty(f, body[1], panes, focus);
    draw_prompt(f, chunks[1], focus);
}

fn draw_sidebar(f: &mut Frame, area: Rect, panes: &[Pane], focus: &LayoutFocus) {
    let items: Vec<ListItem> = panes
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let mark = if p.re_prompt_loading {
                "⏳"
            } else if p.status == PaneStatus::WaitingPrompt {
                "?"
            } else if p.role == AgentRole::Conductor {
                "♪"
            } else {
                p.status.sidebar_mark(focus.tick)
            };
            let style = if i == focus.sidebar_idx {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if p.status == PaneStatus::WaitingPrompt {
                Style::default().fg(Color::Yellow)
            } else {
                status_style(p.status)
            };
            ListItem::new(Line::from(Span::styled(
                format!("{mark} {}", p.block_key),
                style,
            )))
        })
        .collect();

    let title = if focus.conductor_mode {
        " conductor "
    } else if focus.allow_new {
        " agents (n new) "
    } else {
        " agents "
    };
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(if focus.prompt_focused || focus.attached {
                Style::default()
            } else {
                Style::default().fg(Color::Cyan)
            }),
    );
    f.render_widget(list, area);
}

fn draw_pty(f: &mut Frame, area: Rect, panes: &[Pane], focus: &LayoutFocus) {
    let view_only = panes
        .get(focus.sidebar_idx)
        .is_some_and(|p| focus.conductor_mode && p.role != AgentRole::Conductor);
    let title = if focus.attached {
        " PTY · Ctrl+] detach "
    } else if view_only {
        " PTY · view-only "
    } else {
        " PTY · Enter attach "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(if focus.attached {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        });
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(pane) = panes.get(focus.sidebar_idx) else {
        let empty = if focus.conductor_mode {
            "No conductor yet. Type a mission below, Enter to start."
        } else {
            "No agents yet. Press n to create one."
        };
        f.render_widget(Paragraph::new(empty), inner);
        return;
    };

    let lines = pane.screen_lines(inner.height, inner.width);
    let text: Vec<Line> = lines.into_iter().map(Line::from).collect();
    f.render_widget(Paragraph::new(text), inner);
}

fn draw_prompt(f: &mut Frame, area: Rect, focus: &LayoutFocus) {
    let title = if focus.prompt_focused {
        if focus.conductor_mode {
            " conductor mission (Enter start · Esc cancel) "
        } else {
            " re-prompt (Enter send · Esc cancel) "
        }
    } else if focus.conductor_mode {
        " Enter attach conductor · x close · Q quit "
    } else {
        " p = re-prompt · Q quit · hjkl/↑↓ nav "
    };
    let border = if focus.prompt_focused {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let display = if focus.prompt_focused {
        format!("> {}_", focus.prompt)
    } else if !focus.hint.is_empty() {
        focus.hint.clone()
    } else if focus.conductor_mode {
        "Nav: j/k · Enter attach conductor · ensemble view-only · Q quit".into()
    } else {
        "Nav: j/k or ↑↓ · Enter attach · x close · p prompt · Q quit".into()
    };
    let p = Paragraph::new(display)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border),
        );
    f.render_widget(p, area);
}

fn status_style(s: PaneStatus) -> Style {
    match s {
        PaneStatus::Done => Style::default().fg(Color::Green),
        PaneStatus::Failed | PaneStatus::TimedOut => Style::default().fg(Color::Red),
        PaneStatus::WaitingPrompt => Style::default().fg(Color::Yellow),
        PaneStatus::Running | PaneStatus::Starting => Style::default().fg(Color::Cyan),
        PaneStatus::Closed => Style::default().fg(Color::DarkGray),
    }
}
