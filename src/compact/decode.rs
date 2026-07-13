use crate::compact::dict::DictSession;
use std::path::{Path, PathBuf};

pub fn is_dict_ref(s: &str) -> bool {
    let Some(rest) = s.strip_prefix('p') else {
        return s.strip_prefix('s').is_some_and(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_digit()));
    };
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
}

/// Expand `p1` dict ref or return path unchanged.
pub fn expand_path(session: &DictSession, arg: &str) -> String {
    if let Some(path) = session.paths.get(arg) {
        return path.clone();
    }
    arg.to_string()
}

pub fn resolve_file_arg(session: &DictSession, arg: &str) -> PathBuf {
    PathBuf::from(expand_path(session, arg))
}

pub fn resolve_file_arg_with_root(
    session: &DictSession,
    repo_root: &Path,
    arg: &str,
) -> PathBuf {
    let expanded = expand_path(session, arg);
    let p = Path::new(&expanded);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        repo_root.join(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compact::dict::DictSession;

    #[test]
    fn expand_path_from_session() {
        let mut session = DictSession::default();
        session.paths.insert("p1".into(), "src/auth.rs".into());
        assert_eq!(expand_path(&session, "p1"), "src/auth.rs");
        assert_eq!(expand_path(&session, "src/auth.rs"), "src/auth.rs");
    }

    #[test]
    fn is_dict_ref_detects_p_and_s() {
        assert!(is_dict_ref("p1"));
        assert!(is_dict_ref("s42"));
        assert!(!is_dict_ref("src/auth.rs"));
    }
}
