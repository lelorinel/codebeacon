//! Multi-agent TUI shell: sidebar + focused PTY + re-prompt.
//!
//! Used by `codebeacon run-plan` (default) and `codebeacon multi-agent`.

pub mod conductor;
mod detect;
mod keys;
mod layout;
mod pane;

pub use conductor::{AgentRole, SessionMode};
pub use keys::{attach_action, key_to_pty_bytes, nav_action, AttachAction, FocusTarget, NavAction};
pub use pane::{spawn_pane, Pane, PaneJob, PaneStatus};

use crate::locks::{SessionStatus, SharedLockStore};
use crate::run_plan::spawn::{
    build_agent_argv, mission_prompt, write_mcp_config, AgentArgvOpts, RunPlanProvider,
};
use anyhow::{bail, Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::collections::VecDeque;
use std::io::{self, IsTerminal, stdout};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub struct SessionJob {
    pub block_key: String,
    pub brief_path: Option<PathBuf>,
    pub signal_path: PathBuf,
    /// Pre-built prompt (mission or freeform).
    pub prompt: String,
    pub role: AgentRole,
}

impl SessionJob {
    pub fn gallery(block_key: String, signal_path: PathBuf, prompt: String) -> Self {
        Self {
            block_key,
            brief_path: None,
            signal_path,
            prompt,
            role: AgentRole::Ensemble,
        }
    }
}

pub struct SessionOpts {
    pub workspace: PathBuf,
    pub provider: RunPlanProvider,
    pub model: String,
    pub parallel: usize,
    pub dry_run: bool,
    pub mcp_config: Option<PathBuf>,
    pub store: SharedLockStore,
    /// When true, `n` creates a new freeform agent pane (Gallery).
    pub allow_new: bool,
    pub mode: SessionMode,
    /// Pre-selected mode skips the startup picker when set from CLI.
    pub mode_from_cli: bool,
    pub jobs: Vec<SessionJob>,
}

/// Run the multi-agent TUI until the user quits with `Q`.
pub fn run_session(opts: SessionOpts) -> Result<()> {
    let SessionOpts {
        workspace,
        provider,
        model,
        parallel,
        dry_run,
        mcp_config,
        store,
        allow_new,
        mut mode,
        mode_from_cli,
        jobs,
    } = opts;

    if dry_run {
        for job in &jobs {
            println!(
                "[dry-run] TUI would open pane={} signal={}",
                job.block_key,
                job.signal_path.display()
            );
            let _ = store.session_done(&job.block_key, "dry-run", true);
        }
        if jobs.is_empty() && allow_new {
            println!(
                "[dry-run] multi-agent TUI ({})",
                if mode_from_cli {
                    mode.as_str()
                } else {
                    "mode picker"
                }
            );
        }
        return Ok(());
    }

    if !(io::stdin().is_terminal() && io::stdout().is_terminal()) {
        bail!("TUI requires a TTY; use `run-plan --headless` for CI");
    }

    enable_raw_mode().context("enable raw mode")?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture).context("enter alt screen")?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend).context("terminal")?;

    let picker_result = if allow_new && !mode_from_cli {
        pick_session_mode(&mut terminal)
    } else {
        Ok(Some(mode))
    };

    let ui = (|| -> Result<()> {
        let Some(picked) = picker_result? else {
            return Ok(());
        };
        mode = picked;

        let ma_session = if mode == SessionMode::Conductor {
            let session_id = conductor::new_session_id();
            let dir = conductor::session_dir(&workspace, &session_id);
            conductor::write_meta(
                &dir,
                &conductor::SessionMeta {
                    session_id: session_id.clone(),
                    mode: SessionMode::Conductor,
                    conductor_key: "conductor".into(),
                    provider: provider.as_str().into(),
                    model: model.clone(),
                },
            )?;
            conductor::set_active_session(&workspace, &session_id)?;
            Some((session_id, dir))
        } else {
            None
        };

        let opts_ref = SessionOpts {
            workspace: workspace.clone(),
            provider,
            model: model.clone(),
            parallel,
            dry_run: false,
            mcp_config: mcp_config.clone(),
            store: store.clone(),
            allow_new: allow_new && mode == SessionMode::Gallery,
            mode,
            mode_from_cli,
            jobs: vec![],
        };

        let mut queue: VecDeque<SessionJob> = jobs.into();
        let parallel = if parallel == 0 { 8 } else { parallel.max(1) };

        let mut panes: Vec<Pane> = Vec::new();
        while panes.len() < parallel {
            let Some(job) = queue.pop_front() else {
                break;
            };
            panes.push(start_job(&opts_ref, job, panes.len(), ma_session.as_ref())?);
        }

        let result = run_ui_loop(
            &mut terminal,
            &mut panes,
            &mut queue,
            &opts_ref,
            parallel,
            ma_session.as_ref(),
        );

        if ma_session.is_some() {
            conductor::clear_active_session(&workspace);
        }
        result
    })();

    disable_raw_mode().ok();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .ok();
    terminal.show_cursor().ok();

    ui
}

fn pick_session_mode(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<Option<SessionMode>> {
    let mut idx: usize = 0;
    let options = [
        (SessionMode::Gallery, "Gallery — equal panes, press n to create agents"),
        (
            SessionMode::Conductor,
            "Conductor — lead agent spawns ensemble via MCP",
        ),
    ];
    loop {
        terminal.draw(|f| {
            let area = f.area();
            let lines: Vec<ratatui::text::Line> = options
                .iter()
                .enumerate()
                .map(|(i, (_, label))| {
                    let mark = if i == idx { ">" } else { " " };
                    ratatui::text::Line::from(format!("{mark} {label}"))
                })
                .collect();
            let p = ratatui::widgets::Paragraph::new(lines).block(
                ratatui::widgets::Block::default()
                    .borders(ratatui::widgets::Borders::ALL)
                    .title(" multi-agent mode (j/k · Enter · Esc cancel) "),
            );
            f.render_widget(p, area);
        })?;

        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press && key.kind != KeyEventKind::Repeat {
                continue;
            }
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    idx = (idx + 1).min(options.len() - 1);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    idx = idx.saturating_sub(1);
                }
                KeyCode::Enter => return Ok(Some(options[idx].0)),
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(None),
                _ => {}
            }
        }
    }
}

fn mcp_config_for_job(
    opts: &SessionOpts,
    session_id: Option<&str>,
    role: AgentRole,
    block_key: &str,
) -> Result<Option<PathBuf>> {
    let Some(session_id) = session_id else {
        return Ok(opts.mcp_config.clone());
    };
    if opts.provider != RunPlanProvider::Claude {
        // Cursor/Codex use project MCP; env still set on PTY. Session ACTIVE file
        // lets serve discover conductor mode.
        return Ok(opts.mcp_config.clone());
    }
    let dir = conductor::session_dir(&opts.workspace, session_id);
    let path = dir.join(format!("mcp-{block_key}.json"));
    let env = conductor::ma_env(session_id, role, block_key);
    write_mcp_config(&path, &opts.workspace, &env)?;
    Ok(Some(path))
}

fn start_job(
    opts: &SessionOpts,
    job: SessionJob,
    pane_idx: usize,
    ma_session: Option<&(String, PathBuf)>,
) -> Result<Pane> {
    let _ = opts
        .store
        .register_session(&job.block_key, Some(format!("pane-{pane_idx}")));

    let session_id = ma_session.map(|(id, _)| id.as_str());
    let env = if let Some(id) = session_id {
        conductor::ma_env(id, job.role, &job.block_key)
    } else {
        vec![]
    };

    if let Some((_, dir)) = ma_session {
        let _ = conductor::upsert_agent(
            dir,
            conductor::AgentRecord {
                block_key: job.block_key.clone(),
                role: job.role,
                status: "running".into(),
                summary: String::new(),
            },
        );
    }

    let mcp = mcp_config_for_job(opts, session_id, job.role, &job.block_key)?;
    let argv = build_agent_argv(&AgentArgvOpts {
        provider: opts.provider,
        workspace: &opts.workspace,
        model: &opts.model,
        prompt: &job.prompt,
        mcp_config: mcp.as_deref(),
        interactive: true,
    })?;
    spawn_pane(PaneJob {
        block_key: job.block_key,
        argv,
        working_dir: opts.workspace.clone(),
        signal_path: job.signal_path,
        env,
        role: job.role,
    })
    .map_err(|e| anyhow::anyhow!(e))
}

fn run_ui_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    panes: &mut Vec<Pane>,
    queue: &mut VecDeque<SessionJob>,
    opts: &SessionOpts,
    parallel: usize,
    ma_session: Option<&(String, PathBuf)>,
) -> Result<()> {
    let mut sidebar_idx: usize = 0;
    let mut attached = false;
    let mut prompt_focused = false;
    let mut prompt = String::new();
    let mut hint = String::new();
    let mut tick: u64 = 0;
    let mut last_poll = Instant::now();
    let mut quit_armed = false;
    let mut awaiting_new = false;
    let conductor_mode = opts.mode == SessionMode::Conductor;

    if conductor_mode && panes.is_empty() {
        prompt_focused = true;
        awaiting_new = true;
        hint = "type conductor mission, Enter to start".into();
    }

    loop {
        if last_poll.elapsed() >= Duration::from_millis(200) {
            poll_completion(panes, &opts.store, ma_session);
            poll_awaiting_input(panes, opts.provider);
            maybe_start_queued(panes, queue, opts, parallel, ma_session)?;
            if let Some(sess) = ma_session {
                drain_conductor_queue(panes, opts, sess)?;
            }
            last_poll = Instant::now();
            tick = tick.wrapping_add(1);
            if attached {
                if panes
                    .get(sidebar_idx)
                    .is_some_and(|p| p.status.is_terminal())
                {
                    attached = false;
                    hint = "agent finished — Ctrl+] or arrows to navigate".into();
                }
            }
        }

        if sidebar_idx >= panes.len() && !panes.is_empty() {
            sidebar_idx = panes.len() - 1;
        }

        let focus = layout::LayoutFocus {
            sidebar_idx,
            attached,
            prompt_focused,
            prompt: prompt.clone(),
            tick,
            allow_new: opts.allow_new,
            conductor_mode,
            hint: hint.clone(),
        };
        terminal.draw(|f| layout::draw(f, panes, &focus))?;

        if let Some(pane) = panes.get_mut(sidebar_idx) {
            let size = terminal.size()?;
            let rows = size.height.saturating_sub(5).max(5);
            let cols = size.width.saturating_sub(24).max(40);
            pane.resize(rows, cols);
        }

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat => {
                if attached {
                    match attach_action(key) {
                        AttachAction::Detach => {
                            attached = false;
                            hint = "detached".into();
                        }
                        AttachAction::PassThrough => {
                            let bytes = key_to_pty_bytes(key);
                            if let Some(pane) = panes.get_mut(sidebar_idx) {
                                pane.write_bytes(&bytes);
                            }
                        }
                    }
                    continue;
                }

                let target = if prompt_focused {
                    FocusTarget::Prompt
                } else {
                    FocusTarget::Sidebar
                };
                match nav_action(key, target) {
                    NavAction::SidebarUp => {
                        if !panes.is_empty() {
                            sidebar_idx = sidebar_idx.saturating_sub(1);
                        }
                        quit_armed = false;
                    }
                    NavAction::SidebarDown => {
                        if !panes.is_empty() {
                            sidebar_idx = (sidebar_idx + 1).min(panes.len() - 1);
                        }
                        quit_armed = false;
                    }
                    NavAction::Attach => {
                        if let Some(pane) = panes.get(sidebar_idx) {
                            if conductor_mode && pane.role != AgentRole::Conductor {
                                attached = false;
                                hint = "ensemble is view-only — select ♪ conductor to attach".into();
                            } else {
                                attached = true;
                                prompt_focused = false;
                                hint.clear();
                            }
                        }
                        quit_armed = false;
                    }
                    NavAction::NewPane => {
                        if opts.allow_new {
                            prompt_focused = true;
                            awaiting_new = true;
                            prompt.clear();
                            hint = "type agent prompt, Enter to spawn".into();
                        } else if conductor_mode {
                            hint = "conductor mode: use spawn_agent MCP (n disabled)".into();
                        } else {
                            hint = "n only in multi-agent gallery mode".into();
                        }
                        quit_armed = false;
                    }
                    NavAction::ClosePane => {
                        if sidebar_idx < panes.len() {
                            let closing_conductor = conductor_mode
                                && panes[sidebar_idx].role == AgentRole::Conductor;
                            if closing_conductor {
                                if !quit_armed {
                                    quit_armed = true;
                                    hint = "closing conductor quits — press x or Q again".into();
                                    continue;
                                }
                                break;
                            }
                            let key = panes[sidebar_idx].block_key.clone();
                            let role = panes[sidebar_idx].role;
                            panes[sidebar_idx].kill();
                            if !opts.store.session_is_terminal(&key).unwrap_or(false) {
                                let _ = opts.store.session_done(&key, "closed by user", false);
                            }
                            if let Some((_, dir)) = ma_session {
                                let _ = conductor::upsert_agent(
                                    dir,
                                    conductor::AgentRecord {
                                        block_key: key.clone(),
                                        role,
                                        status: "closed".into(),
                                        summary: "closed by user".into(),
                                    },
                                );
                            }
                            panes.remove(sidebar_idx);
                            if sidebar_idx > 0 && sidebar_idx >= panes.len() {
                                sidebar_idx -= 1;
                            }
                            maybe_start_queued(panes, queue, opts, parallel, ma_session)?;
                            hint = format!("closed {key}");
                        }
                        quit_armed = false;
                    }
                    NavAction::FocusPrompt => {
                        if conductor_mode {
                            if panes
                                .get(sidebar_idx)
                                .is_some_and(|p| p.role != AgentRole::Conductor)
                            {
                                hint = "re-prompt only on conductor".into();
                            } else {
                                prompt_focused = true;
                                awaiting_new = false;
                                prompt.clear();
                                hint.clear();
                            }
                        } else {
                            prompt_focused = true;
                            awaiting_new = false;
                            prompt.clear();
                            hint.clear();
                        }
                        quit_armed = false;
                    }
                    NavAction::PromptCancel => {
                        prompt_focused = false;
                        awaiting_new = false;
                        prompt.clear();
                        quit_armed = false;
                    }
                    NavAction::PromptChar(c) => {
                        prompt.push(c);
                        quit_armed = false;
                    }
                    NavAction::PromptBackspace => {
                        prompt.pop();
                        quit_armed = false;
                    }
                    NavAction::PromptSubmit => {
                        let text = prompt.trim().to_string();
                        prompt_focused = false;
                        prompt.clear();
                        if text.is_empty() {
                            continue;
                        }
                        if conductor_mode && panes.is_empty() && awaiting_new {
                            awaiting_new = false;
                            match start_conductor(opts, &text, ma_session) {
                                Ok(p) => {
                                    panes.push(p);
                                    sidebar_idx = 0;
                                    hint = "conductor started".into();
                                }
                                Err(e) => {
                                    hint = format!("spawn failed: {e}");
                                    prompt_focused = true;
                                    awaiting_new = true;
                                }
                            }
                            quit_armed = false;
                            continue;
                        }
                        let spawn_new = opts.allow_new && (panes.is_empty() || awaiting_new);
                        awaiting_new = false;
                        if spawn_new {
                            let block_key = unique_key(panes, "agent");
                            let signal = empty_session_dir(&opts.workspace)
                                .join(format!("DONE.{block_key}"));
                            let job = SessionJob::gallery(block_key.clone(), signal, text);
                            match start_job(opts, job, panes.len(), None) {
                                Ok(p) => {
                                    panes.push(p);
                                    sidebar_idx = panes.len() - 1;
                                    hint = format!("started {block_key}");
                                }
                                Err(e) => hint = format!("spawn failed: {e}"),
                            }
                        } else if let Some(pane) = panes.get_mut(sidebar_idx) {
                            if conductor_mode && pane.role != AgentRole::Conductor {
                                hint = "cannot re-prompt ensemble".into();
                            } else {
                                pane.re_prompt_loading = true;
                                if pane.status.is_terminal() {
                                    pane.status = PaneStatus::Running;
                                }
                                let msg = format!("{text}\r");
                                pane.write_bytes(msg.as_bytes());
                                pane.inject_line(&format!("[codebeacon] re-prompt: {text}"));
                                hint = "re-prompt sent".into();
                            }
                        }
                        quit_armed = false;
                    }
                    NavAction::Quit => {
                        if quit_armed {
                            break;
                        }
                        quit_armed = true;
                        hint = "press Q again to quit".into();
                    }
                    NavAction::None => {
                        quit_armed = false;
                    }
                }
            }
            Event::Mouse(m) => {
                if attached {
                    continue;
                }
                if matches!(
                    m.kind,
                    MouseEventKind::Down(_) | MouseEventKind::Up(_)
                ) {
                    if m.column < 22 && m.row >= 1 {
                        let idx = (m.row as usize).saturating_sub(1);
                        if idx < panes.len() {
                            if idx == sidebar_idx {
                                if conductor_mode
                                    && panes[idx].role != AgentRole::Conductor
                                {
                                    hint = "ensemble is view-only".into();
                                } else {
                                    attached = true;
                                }
                            } else {
                                sidebar_idx = idx;
                            }
                        }
                    }
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
    for pane in panes.iter_mut() {
        pane.kill();
    }
    Ok(())
}

fn start_conductor(
    opts: &SessionOpts,
    mission: &str,
    ma_session: Option<&(String, PathBuf)>,
) -> Result<Pane> {
    let Some((session_id, dir)) = ma_session else {
        bail!("internal: conductor session missing");
    };
    let block_key = "conductor".to_string();
    let signal = dir.join(format!("DONE.{block_key}"));
    let brief = conductor::conductor_brief(&block_key, &signal);
    let prompt = format!("{brief}\n\nMission:\n{mission}");
    start_job(
        opts,
        SessionJob {
            block_key,
            brief_path: None,
            signal_path: signal,
            prompt,
            role: AgentRole::Conductor,
        },
        0,
        Some(&(session_id.clone(), dir.clone())),
    )
}

fn drain_conductor_queue(
    panes: &mut Vec<Pane>,
    opts: &SessionOpts,
    ma_session: &(String, PathBuf),
) -> Result<()> {
    let (_session_id, dir) = ma_session;
    let pending = conductor::drain_spawn_queue(dir).map_err(|e| anyhow::anyhow!(e))?;
    for req in pending {
        let model = req
            .model
            .clone()
            .unwrap_or_else(|| opts.model.clone());
        let block_key = req
            .block_key
            .clone()
            .unwrap_or_else(|| unique_key(panes, "ensemble"));
        if panes.iter().any(|p| p.block_key == block_key) {
            continue;
        }
        let signal = dir.join(format!("DONE.{block_key}"));
        let prompt = conductor::ensemble_brief(&block_key, &signal, &req.prompt);
        let job = SessionJob {
            block_key,
            brief_path: None,
            signal_path: signal,
            prompt,
            role: AgentRole::Ensemble,
        };
        let pane = start_job_with_model(opts, job, panes.len(), Some(ma_session), &model)?;
        panes.push(pane);
    }
    Ok(())
}

fn start_job_with_model(
    opts: &SessionOpts,
    job: SessionJob,
    pane_idx: usize,
    ma_session: Option<&(String, PathBuf)>,
    model: &str,
) -> Result<Pane> {
    let patched = SessionOpts {
        workspace: opts.workspace.clone(),
        provider: opts.provider,
        model: model.to_string(),
        parallel: opts.parallel,
        dry_run: false,
        mcp_config: opts.mcp_config.clone(),
        store: opts.store.clone(),
        allow_new: opts.allow_new,
        mode: opts.mode,
        mode_from_cli: opts.mode_from_cli,
        jobs: vec![],
    };
    start_job(&patched, job, pane_idx, ma_session)
}

fn unique_key(panes: &[Pane], prefix: &str) -> String {
    let mut n = 1usize;
    loop {
        let k = format!("{prefix}-{n}");
        if !panes.iter().any(|p| p.block_key == k) {
            return k;
        }
        n += 1;
    }
}

fn poll_awaiting_input(panes: &mut [Pane], provider: RunPlanProvider) {
    for pane in panes.iter_mut() {
        if pane.status.is_terminal() || pane.re_prompt_loading {
            continue;
        }
        let tail = pane.screen_text_tail(12);
        let waiting = detect::detect_awaiting_input(&tail, provider);
        match (pane.status, waiting) {
            (PaneStatus::WaitingPrompt, false) => {
                pane.status = PaneStatus::Running;
            }
            (PaneStatus::Running | PaneStatus::Starting, true) => {
                pane.status = PaneStatus::WaitingPrompt;
            }
            (PaneStatus::WaitingPrompt, true) => {}
            _ => {}
        }
    }
}

fn poll_completion(
    panes: &mut [Pane],
    store: &SharedLockStore,
    ma_session: Option<&(String, PathBuf)>,
) {
    for pane in panes.iter_mut() {
        if pane.status.is_terminal() {
            continue;
        }
        if let Ok(true) = store.session_is_terminal(&pane.block_key) {
            let ok = store.session_succeeded(&pane.block_key).unwrap_or(false);
            pane.status = if ok {
                PaneStatus::Done
            } else {
                PaneStatus::Failed
            };
            pane.via = "session_done".into();
            pane.re_prompt_loading = false;
            pane.inject_line(&format!(
                "[codebeacon] {}",
                if ok { "done" } else { "failed" }
            ));
            sync_agent_status(ma_session, pane, if ok { "done" } else { "failed" }, "");
            continue;
        }
        if pane.signal_path.exists() {
            if !store
                .session_is_terminal(&pane.block_key)
                .unwrap_or(false)
            {
                let _ = store.session_done(&pane.block_key, "signal file", true);
            }
            pane.status = PaneStatus::Done;
            pane.via = "signal".into();
            pane.re_prompt_loading = false;
            pane.inject_line("[codebeacon] done (signal)");
            sync_agent_status(ma_session, pane, "done", "signal");
            continue;
        }
        if let Some(code) = pane.try_wait_child() {
            let ok = code == 0;
            if !store
                .session_is_terminal(&pane.block_key)
                .unwrap_or(false)
            {
                let _ = store.session_done(
                    &pane.block_key,
                    if ok { "process exit 0" } else { "process failed" },
                    ok,
                );
            }
            pane.exit_code = code;
            pane.status = if ok {
                PaneStatus::Done
            } else {
                PaneStatus::Failed
            };
            pane.via = "exit".into();
            pane.re_prompt_loading = false;
            pane.inject_line(&format!("[codebeacon] exit {code}"));
            sync_agent_status(
                ma_session,
                pane,
                if ok { "done" } else { "failed" },
                &format!("exit {code}"),
            );
        }
    }
}

fn sync_agent_status(
    ma_session: Option<&(String, PathBuf)>,
    pane: &Pane,
    status: &str,
    summary: &str,
) {
    let Some((_, dir)) = ma_session else {
        return;
    };
    let _ = conductor::upsert_agent(
        dir,
        conductor::AgentRecord {
            block_key: pane.block_key.clone(),
            role: pane.role,
            status: status.into(),
            summary: summary.into(),
        },
    );
}

fn maybe_start_queued(
    panes: &mut Vec<Pane>,
    queue: &mut VecDeque<SessionJob>,
    opts: &SessionOpts,
    parallel: usize,
    ma_session: Option<&(String, PathBuf)>,
) -> Result<()> {
    loop {
        let active = panes.iter().filter(|p| !p.status.is_terminal()).count();
        if active >= parallel || queue.is_empty() {
            break;
        }
        let Some(job) = queue.pop_front() else {
            break;
        };
        let pane = start_job(opts, job, panes.len(), ma_session)?;
        panes.push(pane);
    }
    Ok(())
}

/// Build session jobs from run-plan briefs.
pub fn jobs_from_briefs(
    briefs: &[(crate::run_plan::PlanDoc, PathBuf, PathBuf)],
) -> Vec<SessionJob> {
    briefs
        .iter()
        .map(|(plan, brief, signal)| SessionJob {
            block_key: plan.block_key.clone(),
            brief_path: Some(brief.clone()),
            signal_path: signal.clone(),
            prompt: mission_prompt(brief, &plan.block_key, signal),
            role: AgentRole::Ensemble,
        })
        .collect()
}

/// Helper used by multi-agent empty session / gallery signals.
pub fn empty_session_dir(workspace: &Path) -> PathBuf {
    let dir = conductor::multi_agent_root(workspace);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[allow(dead_code)]
fn _status_label(s: SessionStatus) -> &'static str {
    match s {
        SessionStatus::Running => "running",
        SessionStatus::Done => "done",
        SessionStatus::Failed => "failed",
        SessionStatus::TimedOut => "timed_out",
    }
}
