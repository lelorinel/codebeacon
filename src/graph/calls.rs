//! Intra-repo call graph (symbol → symbol), separate from file import graph.

use petgraph::graph::DiGraph;
use petgraph::prelude::NodeIndex;
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CallNode {
    pub file: String,
    pub symbol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallEdge {
    pub caller_file: String,
    pub caller_sym: String,
    pub callee_name: String,
    pub callee_file: Option<String>,
    pub line: u32,
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CallGraphStore {
    pub edges: Vec<CallEdge>,
}

#[derive(Debug, Default)]
pub struct CallGraph {
    graph: DiGraph<CallNode, u32>,
    node_map: HashMap<CallNode, NodeIndex>,
    /// Unresolved callee name → caller nodes
    unresolved: HashMap<String, Vec<(CallNode, u32)>>,
}

impl CallGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_store(store: &CallGraphStore) -> Self {
        let mut g = Self::new();
        for e in &store.edges {
            let caller = CallNode {
                file: e.caller_file.clone(),
                symbol: e.caller_sym.clone(),
            };
            if e.resolved {
                if let Some(ref cf) = e.callee_file {
                    let callee = CallNode {
                        file: cf.clone(),
                        symbol: e.callee_name.clone(),
                    };
                    g.add_edge(caller, callee, e.line);
                    continue;
                }
            }
            g.unresolved
                .entry(e.callee_name.clone())
                .or_default()
                .push((caller, e.line));
        }
        g
    }

    fn get_or_insert(&mut self, node: CallNode) -> NodeIndex {
        if let Some(&idx) = self.node_map.get(&node) {
            return idx;
        }
        let idx = self.graph.add_node(node.clone());
        self.node_map.insert(node, idx);
        idx
    }

    pub fn add_edge(&mut self, caller: CallNode, callee: CallNode, line: u32) {
        let a = self.get_or_insert(caller);
        let b = self.get_or_insert(callee);
        self.graph.add_edge(a, b, line);
    }

    pub fn callers(&self, symbol: &str, file_hint: Option<&str>, depth: u32) -> Vec<CallNode> {
        self.walk(symbol, file_hint, depth, Direction::Incoming)
    }

    pub fn callees(&self, symbol: &str, file_hint: Option<&str>, depth: u32) -> Vec<CallNode> {
        self.walk(symbol, file_hint, depth, Direction::Outgoing)
    }

    fn walk(
        &self,
        symbol: &str,
        file_hint: Option<&str>,
        depth: u32,
        dir: Direction,
    ) -> Vec<CallNode> {
        let seeds: Vec<NodeIndex> = self
            .node_map
            .iter()
            .filter(|(n, _)| {
                n.symbol == symbol
                    && file_hint
                        .map(|f| n.file == f || n.file.ends_with(f))
                        .unwrap_or(true)
            })
            .map(|(_, &idx)| idx)
            .collect();

        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        let mut frontier: Vec<(NodeIndex, u32)> =
            seeds.into_iter().map(|i| (i, 0)).collect();

        while let Some((idx, d)) = frontier.pop() {
            if d >= depth {
                continue;
            }
            for neigh in self.graph.neighbors_directed(idx, dir) {
                if seen.insert(neigh) {
                    out.push(self.graph[neigh].clone());
                    frontier.push((neigh, d + 1));
                }
            }
        }
        out.sort_by(|a, b| (&a.file, &a.symbol).cmp(&(&b.file, &b.symbol)));
        out
    }

    pub fn fan_in(&self, symbol: &str) -> usize {
        self.callers(symbol, None, 1).len()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }
}

/// Extract call sites from source with regex heuristics (works without tree-sitter).
pub fn extract_calls_regex(
    code: &str,
    file: &str,
    function_symbols: &[(String, u32)],
) -> Vec<CallEdge> {
    let call_re = regex::Regex::new(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\(").expect("call re");
    let mut edges = Vec::new();
    let mut funcs: Vec<(String, u32)> = function_symbols.to_vec();
    funcs.sort_by_key(|(_, line)| *line);

    for (line_num, line) in code.lines().enumerate() {
        let line_no = (line_num + 1) as u32;
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with('*') {
            continue;
        }
        // Skip definition lines themselves
        if trimmed.contains("fn ") || trimmed.starts_with("def ") || trimmed.contains("function ") {
            continue;
        }
        let caller = enclosing_fn(&funcs, line_no);
        let Some(caller_sym) = caller else {
            continue;
        };
        for caps in call_re.captures_iter(line) {
            let callee = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            if callee.is_empty()
                || callee == caller_sym
                || is_keyword(callee)
                || callee.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
                    && callee.len() <= 3
            {
                continue;
            }
            edges.push(CallEdge {
                caller_file: file.to_string(),
                caller_sym: caller_sym.clone(),
                callee_name: callee.to_string(),
                callee_file: None,
                line: line_no,
                resolved: false,
            });
        }
    }
    edges
}

fn enclosing_fn(funcs: &[(String, u32)], line: u32) -> Option<String> {
    let mut best: Option<&(String, u32)> = None;
    for f in funcs {
        if f.1 <= line {
            best = Some(f);
        } else {
            break;
        }
    }
    best.map(|(n, _)| n.clone())
}

fn is_keyword(s: &str) -> bool {
    matches!(
        s,
        "if" | "for"
            | "while"
            | "match"
            | "return"
            | "let"
            | "mut"
            | "async"
            | "await"
            | "self"
            | "Self"
            | "super"
            | "crate"
            | "use"
            | "mod"
            | "pub"
            | "fn"
            | "impl"
            | "struct"
            | "enum"
            | "trait"
            | "type"
            | "const"
            | "static"
            | "where"
            | "unsafe"
            | "loop"
            | "break"
            | "continue"
            | "else"
            | "print"
            | "println"
            | "format"
            | "vec"
            | "Some"
            | "None"
            | "Ok"
            | "Err"
            | "true"
            | "false"
            | "new"
            | "from"
            | "into"
            | "as"
            | "isinstance"
            | "len"
            | "range"
            | "require"
            | "import"
            | "export"
            | "class"
            | "def"
            | "lambda"
    )
}

/// Resolve callee names against known symbols (same file → same package → unique global).
pub fn resolve_calls(
    edges: &mut [CallEdge],
    symbols: &HashMap<String, Vec<(String, String)>>, // name → [(file, package)]
) {
    for e in edges.iter_mut() {
        let Some(cands) = symbols.get(&e.callee_name) else {
            continue;
        };
        if let Some((file, _)) = cands.iter().find(|(f, _)| f == &e.caller_file) {
            e.callee_file = Some(file.clone());
            e.resolved = true;
            continue;
        }
        if cands.len() == 1 {
            e.callee_file = Some(cands[0].0.clone());
            e.resolved = true;
        }
    }
}

pub fn save_calls(store: &CallGraphStore, codeindex: &Path) -> anyhow::Result<()> {
    let path = codeindex.join("calls.bin");
    let bytes = bincode::serialize(store)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

pub fn load_calls(codeindex: &Path) -> CallGraphStore {
    let path = codeindex.join("calls.bin");
    std::fs::read(path)
        .ok()
        .and_then(|b| bincode::deserialize(&b).ok())
        .unwrap_or_default()
}

pub fn build_symbol_index(
    packages: &[crate::types::PackageDetail],
) -> HashMap<String, Vec<(String, String)>> {
    let mut map: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for pkg in packages {
        for file in &pkg.files {
            let path = file.path.to_string_lossy().into_owned();
            for sym in &file.symbols {
                if matches!(
                    sym.kind,
                    crate::types::SymbolKind::Function
                        | crate::types::SymbolKind::Module
                        | crate::types::SymbolKind::Other
                ) {
                    map.entry(sym.name.clone())
                        .or_default()
                        .push((path.clone(), pkg.name.clone()));
                }
            }
        }
    }
    map
}

/// Build call store from package details by re-reading source files.
pub fn build_call_store(
    repo_root: &Path,
    packages: &[crate::types::PackageDetail],
) -> CallGraphStore {
    let mut edges = Vec::new();
    for pkg in packages {
        for file in &pkg.files {
            let abs = repo_root.join(&file.path);
            let code = std::fs::read_to_string(&abs).unwrap_or_default();
            let funcs: Vec<(String, u32)> = file
                .symbols
                .iter()
                .filter(|s| s.kind == crate::types::SymbolKind::Function)
                .map(|s| (s.name.clone(), s.line))
                .collect();
            let path_str = file.path.to_string_lossy();
            edges.extend(extract_calls_regex(&code, &path_str, &funcs));
        }
    }
    let sym_index = build_symbol_index(packages);
    resolve_calls(&mut edges, &sym_index);
    CallGraphStore { edges }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_call_to_find_user() {
        let code = r#"
pub fn login(email: &str) -> Option<String> {
    find_user(email).map(|_| "token".to_string())
}
"#;
        let funcs = vec![("login".into(), 2)];
        let edges = extract_calls_regex(code, "src/auth.rs", &funcs);
        assert!(
            edges.iter().any(|e| e.callee_name == "find_user"),
            "got {:?}",
            edges
        );
    }

    #[test]
    fn call_graph_callers() {
        let mut g = CallGraph::new();
        g.add_edge(
            CallNode {
                file: "a.rs".into(),
                symbol: "login".into(),
            },
            CallNode {
                file: "b.rs".into(),
                symbol: "find_user".into(),
            },
            3,
        );
        let callers = g.callers("find_user", None, 1);
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0].symbol, "login");
    }
}
