//! Heuristic detection of agents waiting for user input (herdr-inspired).
//!
//! Scans the bottom of the PTY screen for permission / question chrome for
//! Cursor, Claude, and Codex — without vendoring herdr's TOML engine.

use crate::run_plan::spawn::RunPlanProvider;

/// True when the visible screen looks like the agent is blocked on the user.
pub fn detect_awaiting_input(screen: &str, provider: RunPlanProvider) -> bool {
    let bottom = bottom_non_empty(screen, 12);
    if bottom.is_empty() {
        return false;
    }
    let joined = bottom.join("\n");
    let lower = joined.to_lowercase();

    if looks_working(&lower, &bottom) {
        return false;
    }

    if provider_blocked(provider, &lower, &bottom) {
        return true;
    }

    soft_question(&bottom)
}

fn bottom_non_empty(screen: &str, n: usize) -> Vec<String> {
    screen
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .rev()
        .take(n)
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn looks_working(lower: &str, lines: &[String]) -> bool {
    if lower.contains("ctrl+c to stop") {
        return true;
    }
    // Codex: • Working (…esc to interrupt)
    if lower.contains("working (") && lower.contains("esc to interrupt") {
        return true;
    }
    // Braille / classic spinners on a line with an -ing verb
    for line in lines.iter().rev().take(8) {
        let t = line.trim_start();
        if t.is_empty() {
            continue;
        }
        let first = t.chars().next().unwrap_or(' ');
        let is_spinner = matches!(first, '⠋' | '⠙' | '⠹' | '⠸' | '⠼' | '⠴' | '⠦' | '⠧' | '⠇' | '⠏'
            | '⬡' | '⬢')
            || ('\u{2800}'..='\u{28FF}').contains(&first);
        if is_spinner {
            let rest = t.chars().skip(1).collect::<String>().to_lowercase();
            if rest.contains("ing") {
                return true;
            }
        }
    }
    false
}

fn provider_blocked(provider: RunPlanProvider, lower: &str, lines: &[String]) -> bool {
    match provider {
        RunPlanProvider::Cursor => cursor_blocked(lower),
        RunPlanProvider::Claude => claude_blocked(lower, lines),
        RunPlanProvider::Codex => codex_blocked(lower),
    }
}

fn cursor_blocked(lower: &str) -> bool {
    lower.contains("waiting for approval")
        || lower.contains("run this command?")
        || lower.contains("write to this file?")
        || lower.contains("(y) (enter)")
        || lower.contains("skip (esc or n)")
        || lower.contains("run (once) (y)")
        || lower.contains("reject & propose changes")
        || lower.contains("proceed (y)")
}

fn claude_blocked(lower: &str, lines: &[String]) -> bool {
    if lower.contains("do you want to proceed?") {
        return true;
    }
    if lower.contains("enter to select") && lower.contains("esc to cancel") {
        return true;
    }
    if lower.contains("run a dynamic workflow?") && lower.contains("esc to cancel") {
        return true;
    }
    // Numbered Yes/No chooser
    let has_yes = lines.iter().any(|l| {
        let t = l.trim().to_lowercase();
        t.starts_with("1. yes")
            || t.starts_with("❯ 1. yes")
            || t.starts_with("❯ yes")
            || t == "yes"
            || t.starts_with("1.yes")
    });
    let has_no = lines.iter().any(|l| {
        let t = l.trim().to_lowercase();
        t.starts_with("2. no") || t.starts_with("❯ 2. no")
    });
    has_yes && has_no
}

fn codex_blocked(lower: &str) -> bool {
    lower.contains("action required")
        || lower.contains("allow command?")
        || lower.contains("enter to submit answer")
        || lower.contains("enter to submit all")
        || lower.contains("press enter to confirm or esc to cancel")
        || lower.contains("[y/n]")
        || lower.contains("yes (y)")
}

/// Conversational questions in the bottom few lines (no working chrome).
fn soft_question(lines: &[String]) -> bool {
    let tail: Vec<&str> = lines.iter().rev().take(6).map(|s| s.as_str()).collect();
    for line in &tail {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let lower = t.to_lowercase();
        if lower.contains("waiting for your")
            || lower.contains("reply with")
            || lower.contains("which option")
            || lower.contains("should i ")
            || lower.contains("do you want")
            || lower.contains("would you like")
        {
            return true;
        }
        // Short question line (avoid long code with ?)
        if t.ends_with('?') && t.len() < 160 && !looks_like_code(t) {
            return true;
        }
    }
    false
}

fn looks_like_code(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("fn ")
        || t.starts_with("pub ")
        || t.starts_with("let ")
        || t.starts_with("const ")
        || t.starts_with("if ")
        || t.starts_with("for ")
        || t.starts_with("while ")
        || t.starts_with("use ")
        || t.starts_with("import ")
        || t.starts_with("//")
        || t.starts_with('#')
        || t.contains("=>")
        || t.contains("::")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_approval_is_waiting() {
        let screen = "\
Something happened
waiting for approval
run this command?
(y) (enter)  skip (esc or n)
";
        assert!(detect_awaiting_input(screen, RunPlanProvider::Cursor));
    }

    #[test]
    fn claude_proceed_is_waiting() {
        let screen = "\
Bash(rm -rf /tmp/x)
Do you want to proceed?
❯ 1. Yes
  2. No
";
        assert!(detect_awaiting_input(screen, RunPlanProvider::Claude));
    }

    #[test]
    fn codex_allow_is_waiting() {
        let screen = "\
Allow command?
press enter to confirm or esc to cancel
";
        assert!(detect_awaiting_input(screen, RunPlanProvider::Codex));
    }

    #[test]
    fn spinner_working_not_waiting() {
        let screen = "\
Analyzing repo
⠋ Thinking about next steps
ctrl+c to stop
";
        assert!(!detect_awaiting_input(screen, RunPlanProvider::Cursor));
    }

    #[test]
    fn soft_question_detected() {
        let screen = "\
I can refactor auth two ways.
Which option should I take?
";
        assert!(detect_awaiting_input(screen, RunPlanProvider::Cursor));
    }

    #[test]
    fn code_question_mark_ignored() {
        let screen = "\
fn maybe_login(user: Option<User>) -> Result<()> {
    // what if user is None?
}
";
        assert!(!detect_awaiting_input(screen, RunPlanProvider::Claude));
    }

    #[test]
    fn empty_not_waiting() {
        assert!(!detect_awaiting_input("", RunPlanProvider::Codex));
    }
}
