//! Query/document tokenization: camelCase / snake_case split, light stemming, stopwords.

const STOPWORDS: &[&str] = &[
    "the", "a", "an", "and", "or", "of", "to", "in", "for", "on", "is", "are", "be",
    "this", "that", "with", "from", "by", "as", "at", "it", "its",
];

/// Tokenize text into stemmed terms suitable for BM25 / search.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in split_raw(text) {
        for part in split_ident(&raw) {
            let lower = part.to_lowercase();
            if lower.len() < 2 || is_stopword(&lower) {
                continue;
            }
            let stemmed = light_stem(&lower);
            // Short prefixes help "auth" match "authentication" / "authenticate".
            // Use char counts — byte slices panic on multi-byte UTF-8 (e.g. Turkish ç).
            if stemmed.chars().count() >= 4 {
                out.push(stemmed.chars().take(4).collect());
            }
            out.push(stemmed);
        }
    }
    out
}

/// Split on non-alphanumeric separators (keeps `/` and `_` boundaries via split_ident).
fn split_raw(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '/')
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Split `userLogin` / `user_login` / `USER_LOGIN` / path segments into parts.
pub fn split_ident(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    for segment in s.split(['_', '/', '-', '.']) {
        if segment.is_empty() {
            continue;
        }
        parts.extend(split_camel(segment));
    }
    if parts.is_empty() && !s.is_empty() {
        parts.push(s.to_string());
    }
    parts
}

fn split_camel(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return vec![];
    }
    let mut parts = Vec::new();
    let mut start = 0;
    for i in 1..chars.len() {
        let prev = chars[i - 1];
        let cur = chars[i];
        let next_lower = chars.get(i + 1).map(|c| c.is_lowercase()).unwrap_or(false);
        let boundary = (prev.is_lowercase() && cur.is_uppercase())
            || (prev.is_uppercase() && cur.is_uppercase() && next_lower)
            || (prev.is_ascii_digit() != cur.is_ascii_digit() && cur.is_alphabetic());
        if boundary {
            parts.push(chars[start..i].iter().collect());
            start = i;
        }
    }
    parts.push(chars[start..].iter().collect());
    parts
}

fn is_stopword(t: &str) -> bool {
    STOPWORDS.contains(&t)
}

/// Very light English stemmer (suffix stripping) — enough for auth/authentication.
pub fn light_stem(word: &str) -> String {
    let w = word;
    let char_len = w.chars().count();
    if char_len <= 3 {
        return w.to_string();
    }
    let suffixes = [
        "ational", "ation", "ations", "eness", "ingly", "edly", "ing", "ies", "ied",
        "ies", "es", "s", "ment", "ness", "ful", "less", "ize", "ise", "ity", "ive",
        "ous", "al", "er", "est", "ly", "ed",
    ];
    for suf in suffixes {
        let suf_chars = suf.chars().count();
        if char_len > suf_chars + 2 && w.ends_with(suf) {
            // Suffixes are ASCII; strip by bytes only when the cut is a char boundary.
            let cut = w.len().saturating_sub(suf.len());
            if w.is_char_boundary(cut) {
                let stem = &w[..cut];
                if stem.chars().count() >= 2 {
                    return stem.to_string();
                }
            }
        }
    }
    w.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel_and_snake_split() {
        assert_eq!(split_ident("userLogin"), vec!["user", "Login"]);
        assert_eq!(split_ident("user_login"), vec!["user", "login"]);
        assert_eq!(split_ident("HTTPServer"), vec!["HTTP", "Server"]);
    }

    #[test]
    fn auth_stems_toward_authentication() {
        let a = tokenize("auth");
        let b = tokenize("authentication");
        assert!(a.iter().any(|t| b.contains(t)) || b.iter().any(|t| a.contains(t)));
    }

    #[test]
    fn login_matches_user_login_tokens() {
        let q = tokenize("login");
        let doc = tokenize("user_login");
        assert!(q.iter().any(|t| doc.contains(t)));
    }

    #[test]
    fn turkish_chars_do_not_panic() {
        let tokens = tokenize("açıklama ölçüm içerik");
        assert!(!tokens.is_empty());
        let _ = tokenize("çğüşöıÇĞÜŞÖİ");
    }
}
