use crate::query::RepoQueryCtx;
use crate::types::{PackageDetail, SymbolEntry, SymbolKind};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ApiExport {
    pub name: String,
    pub kind: String,
    pub file: String,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiSurfaceResponse {
    pub package: String,
    pub exports: Vec<ApiExport>,
    pub internal_count: usize,
}

pub fn api_surface(pkg: &PackageDetail) -> ApiSurfaceResponse {
    let mut exports = Vec::new();
    let mut internal_count = 0usize;

    for file in &pkg.files {
        let path_str = file.path.to_string_lossy().into_owned();
        for sym in &file.symbols {
            if is_public_export(sym, &path_str) {
                exports.push(ApiExport {
                    name: sym.name.clone(),
                    kind: kind_label(&sym.kind),
                    file: path_str.clone(),
                    line: sym.line,
                });
            } else {
                internal_count += 1;
            }
        }
    }

    exports.sort_by(|a, b| a.name.cmp(&b.name));

    ApiSurfaceResponse {
        package: pkg.name.clone(),
        exports,
        internal_count,
    }
}

fn is_public_export(sym: &SymbolEntry, path: &str) -> bool {
    if sym.signature.contains("pub ") || sym.signature.contains("export ") {
        return true;
    }
    if path.ends_with("lib.rs") || path.ends_with("mod.rs") || path.ends_with("index.ts") {
        return matches!(sym.kind, SymbolKind::Function | SymbolKind::Struct | SymbolKind::Enum);
    }
    if sym.name.chars().next().is_some_and(|c| c.is_uppercase()) {
        return matches!(sym.kind, SymbolKind::Function | SymbolKind::Struct);
    }
    false
}

fn kind_label(kind: &SymbolKind) -> String {
    match kind {
        SymbolKind::Function => "fn",
        SymbolKind::Struct => "st",
        SymbolKind::Enum => "en",
        SymbolKind::Trait => "tr",
        SymbolKind::Module => "md",
        SymbolKind::Variable => "vr",
        SymbolKind::Other => "ot",
    }
    .into()
}

#[derive(Debug, Clone, Serialize)]
pub struct WhyFileResponse {
    pub file: String,
    pub package: String,
    pub depends_on: Vec<String>,
    pub depended_by: Vec<String>,
    pub recent_commits: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blame_first_line: Option<String>,
}

pub fn why_file(
    ctx: &RepoQueryCtx,
    rel_path: &str,
    recent_commits: Vec<String>,
    blame_first_line: Option<String>,
) -> WhyFileResponse {
    let package = crate::indexer::package::package_name_for(std::path::Path::new(rel_path));
    let (depends_on, depended_by) = ctx
        .packages
        .values()
        .flat_map(|p| p.files.iter())
        .find(|f| f.path.to_string_lossy() == rel_path)
        .map(|f| (f.depends_on.clone(), f.depended_by.clone()))
        .unwrap_or_default();

    WhyFileResponse {
        file: rel_path.to_string(),
        package,
        depends_on,
        depended_by,
        recent_commits,
        blame_first_line,
    }
}
