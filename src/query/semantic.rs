//! Optional embedding-based semantic search (feature = "embeddings").
//!
//! Without the feature, provides a stub that falls back to BM25 query results.

use crate::query::{QueryMatch, RepoQueryCtx};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SemanticSearchResponse {
    pub query: String,
    pub matches: Vec<QueryMatch>,
    pub backend: String,
}

/// Semantic search: with `embeddings` feature uses in-memory bag-of-char vectors;
/// otherwise falls back to BM25 (`query`).
pub fn semantic_search(
    ctx: &RepoQueryCtx,
    query: &str,
    limit: usize,
) -> SemanticSearchResponse {
    #[cfg(feature = "embeddings")]
    {
        let matches = embed_search(ctx, query, limit);
        return SemanticSearchResponse {
            query: query.to_string(),
            matches,
            backend: "char-ngram-embeddings".into(),
        };
    }
    #[cfg(not(feature = "embeddings"))]
    {
        let matches = ctx.query(query, limit);
        SemanticSearchResponse {
            query: query.to_string(),
            matches,
            backend: "bm25-fallback".into(),
        }
    }
}

#[cfg(feature = "embeddings")]
fn embed_search(ctx: &RepoQueryCtx, query: &str, limit: usize) -> Vec<QueryMatch> {
    use crate::query::bm25::DocKind;
    use crate::query::MatchKind;

    let q = char_ngram_vec(query, 3);
    let mut scored: Vec<(usize, f32)> = Vec::new();
    for (i, doc) in ctx.search.docs.iter().enumerate() {
        let text = format!("{} {}", doc.name, doc.detail);
        let v = char_ngram_vec(&text, 3);
        let s = cosine(&q, &v);
        if s > 0.05 {
            scored.push((i, s));
        }
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    scored
        .into_iter()
        .map(|(i, score)| {
            let doc = &ctx.search.docs[i];
            let kind = match doc.kind {
                DocKind::Package => MatchKind::Package,
                DocKind::File => MatchKind::File,
                DocKind::Symbol => MatchKind::Symbol,
                DocKind::HotSymbol => MatchKind::HotSymbol,
            };
            QueryMatch {
                kind,
                name: doc.name.clone(),
                detail: doc.detail.clone(),
                score,
                hint: doc.hint.clone(),
            }
        })
        .collect()
}

#[cfg(feature = "embeddings")]
fn char_ngram_vec(text: &str, n: usize) -> std::collections::HashMap<String, f32> {
    let lower = text.to_lowercase();
    let chars: Vec<char> = lower.chars().filter(|c| c.is_alphanumeric()).collect();
    let mut map = std::collections::HashMap::new();
    if chars.len() < n {
        map.insert(lower, 1.0);
        return map;
    }
    for i in 0..=chars.len() - n {
        let gram: String = chars[i..i + n].iter().collect();
        *map.entry(gram).or_insert(0.0) += 1.0;
    }
    // L2 normalize
    let norm = map.values().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
    for v in map.values_mut() {
        *v /= norm;
    }
    map
}

#[cfg(feature = "embeddings")]
fn cosine(a: &std::collections::HashMap<String, f32>, b: &std::collections::HashMap<String, f32>) -> f32 {
    let mut sum = 0.0;
    for (k, va) in a {
        if let Some(vb) = b.get(k) {
            sum += va * vb;
        }
    }
    sum
}
