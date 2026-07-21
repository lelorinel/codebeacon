//! Hybrid docs update briefs (beacon does not write markdown).

use crate::docs::anchor::{parse_reference, resolve, Anchor};
use crate::docs::index::{DocsIndex, DocsSection};
use std::path::Path;

/// Build a human/agent-readable brief for updating stale (or selected) doc sections.
/// Does **not** write markdown — the agent applies the brief.
pub fn build_update_brief(
    repo_root: &Path,
    index: &DocsIndex,
    section_id: Option<&str>,
) -> String {
    let sections: Vec<&DocsSection> = if let Some(id) = section_id {
        index
            .sections
            .iter()
            .filter(|s| s.id == id || s.id.contains(id))
            .collect()
    } else {
        index.sections.iter().filter(|s| s.stale).collect()
    };

    if sections.is_empty() {
        return if section_id.is_some() {
            format!(
                "No matching docs section for '{}'. Use docs_status / query_docs to list ids.",
                section_id.unwrap_or("")
            )
        } else {
            "No stale documentation sections. Nothing to update.".into()
        };
    }

    let mut out = String::new();
    out.push_str("# Docs update brief\n\n");
    out.push_str(
        "Update the markdown sections below to match the current code. \
         Write the files yourself; codebeacon will reindex after you save.\n\n",
    );

    for sec in sections {
        out.push_str(&format!("## Section `{}`\n\n", sec.id));
        out.push_str(&format!("- **File:** {}\n", sec.file));
        if !sec.heading.is_empty() {
            out.push_str(&format!("- **Heading:** {}\n", sec.heading));
        }
        out.push_str(&format!(
            "- **Lines:** {}-{}\n",
            sec.start_line, sec.end_line
        ));
        out.push_str(&format!("- **Stale:** {}\n", sec.stale));
        if !sec.snippet.is_empty() {
            out.push_str(&format!("- **Current snippet:** {}\n", sec.snippet));
        }
        out.push_str("\n### Linked code\n\n");
        if sec.links.is_empty() {
            out.push_str("(no links — update from surrounding context)\n\n");
        } else {
            for link in &sec.links {
                out.push_str(&format!(
                    "- `{}` ({:?}{})\n",
                    link.target,
                    link.kind,
                    if link.broken { ", broken" } else { "" }
                ));
                if let Some(excerpt) = code_excerpt(repo_root, &link.target) {
                    out.push_str("```\n");
                    out.push_str(&excerpt);
                    if !excerpt.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push_str("```\n");
                }
            }
            out.push('\n');
        }
        out.push_str("### Instructions\n\n");
        out.push_str(
            "1. Read the linked code (and nearby modules if needed).\n\
             2. Edit this section so it reflects current behaviour.\n\
             3. Keep the heading text stable when possible (`",
        );
        out.push_str(if sec.heading.is_empty() {
            &sec.file
        } else {
            &sec.heading
        });
        out.push_str(
            "`).\n\
             4. Preserve or add `<!-- codebeacon: path -->` links for related files.\n\n",
        );
    }
    out
}

fn code_excerpt(repo_root: &Path, target: &str) -> Option<String> {
    let r = parse_reference(target);
    if r.path.is_empty() {
        return None;
    }
    match resolve(repo_root, &r) {
        Ok(slice) => {
            let mut content = slice.content;
            const MAX: usize = 1200;
            if content.len() > MAX {
                content.truncate(MAX);
                content.push_str("\n…");
            }
            // For whole-file, prefer a shorter peek
            if matches!(r.anchor, Anchor::Whole) && content.lines().count() > 40 {
                let lines: Vec<&str> = content.lines().take(40).collect();
                return Some(format!("{}\n…", lines.join("\n")));
            }
            Some(content)
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docs::index::{reindex_docs, mark_stale_for_code_path, load_docs_index};
    use crate::config::codeindex_dir;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn brief_lists_stale_sections() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/auth.rs"), "pub fn login() { /* ok */ }\n").unwrap();
        fs::write(
            root.join("docs/a.md"),
            "## Auth\n\n<!-- codebeacon: src/auth.rs -->\nOld text.\n",
        )
        .unwrap();
        reindex_docs(root, Path::new("docs"), false).unwrap();
        mark_stale_for_code_path(root, Path::new("src/auth.rs")).unwrap();
        let idx = load_docs_index(&codeindex_dir(root)).unwrap().unwrap();
        let brief = build_update_brief(root, &idx, None);
        assert!(brief.contains("Docs update brief"));
        assert!(brief.contains("## Auth") || brief.contains("Auth"));
        assert!(brief.contains("src/auth.rs"));
    }
}
