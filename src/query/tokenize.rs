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
            if stemmed.len() >= 4 {
                out.push(stemmed[..4].to_string());
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
    if w.len() <= 3 {
        return w.to_string();
    }
    let suffixes = [
        "ational", "ation", "ations", "eness", "ingly", "edly", "ing", "ies", "ied",
        "ies", "es", "s", "ment", "ness", "ful", "less", "ize", "ise", "ity", "ive",
        "ous", "al", "er", "est", "ly", "ed",
    ];
    for suf in suffixes {
        if w.len() > suf.len() + 2 && w.ends_with(suf) {
            let stem = &w[..w.len() - suf.len()];
            if stem.len() >= 2 {
                return stem.to_string();
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
}
