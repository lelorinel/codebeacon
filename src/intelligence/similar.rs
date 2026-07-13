use crate::query::RepoQueryCtx;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SimilarSymbol {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub signature: String,
    pub score: f32,
}

pub fn similar_symbols(
    ctx: &RepoQueryCtx,
    symbol: &str,
    file_hint: Option<&str>,
    limit: usize,
) -> Vec<SimilarSymbol> {
    let target = find_symbol(ctx, symbol, file_hint);
    let Some((target_sym, _)) = target else {
        return vec![];
    };

    let mut out = Vec::new();
    for pkg in ctx.packages.values() {
        for file in &pkg.files {
            for sym in &file.symbols {
                if sym.name == symbol {
                    continue;
                }
                let score = symbol_similarity(target_sym, sym);
                if score < 0.3 {
                    continue;
                }
                out.push(SimilarSymbol {
                    name: sym.name.clone(),
                    file: file.path.to_string_lossy().into_owned(),
                    line: sym.line,
                    signature: sym.signature.clone(),
                    score,
                });
            }
        }
    }

    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out.truncate(limit);
    out
}

fn find_symbol<'a>(
    ctx: &'a RepoQueryCtx,
    symbol: &str,
    file_hint: Option<&str>,
) -> Option<(&'a crate::types::SymbolEntry, String)> {
    for pkg in ctx.packages.values() {
        for file in &pkg.files {
            if let Some(h) = file_hint {
                let p = file.path.to_string_lossy();
                if p != h && !p.ends_with(h) {
                    continue;
                }
            }
            for sym in &file.symbols {
                if sym.name == symbol {
                    return Some((sym, file.path.to_string_lossy().into_owned()));
                }
            }
        }
    }
    None
}

fn symbol_similarity(a: &crate::types::SymbolEntry, b: &crate::types::SymbolEntry) -> f32 {
    let mut score = 0.0f32;
    if a.kind == b.kind {
        score += 0.4;
    }
    let a_params = a.signature.matches('(').count();
    let b_params = b.signature.matches('(').count();
    if a_params == b_params {
        score += 0.2;
    }
    if a.signature.split_whitespace().count() == b.signature.split_whitespace().count() {
        score += 0.2;
    }
    let common: usize = a
        .name
        .chars()
        .zip(b.name.chars())
        .filter(|(x, y)| x == y)
        .count();
    score += (common as f32 / a.name.len().max(b.name.len()).max(1) as f32) * 0.2;
    score
}
