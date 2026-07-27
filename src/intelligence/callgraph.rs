//! Call-graph intelligence: callers/callees + impact enrichment.

use crate::config::codeindex_dir;
use crate::graph::calls::{load_calls, CallGraph, CallGraphStore, CallNode};
use crate::query::RepoQueryCtx;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CallGraphResponse {
    pub symbol: String,
    pub callers: Vec<CallRef>,
    pub callees: Vec<CallRef>,
    pub depth: u32,
    pub edge_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CallRef {
    pub file: String,
    pub symbol: String,
}

pub fn load_call_graph(ctx: &RepoQueryCtx) -> CallGraph {
    let store = load_calls(&codeindex_dir(&ctx.root));
    CallGraph::from_store(&store)
}

pub fn call_graph(
    ctx: &RepoQueryCtx,
    symbol: &str,
    file_hint: Option<&str>,
    direction: &str,
    depth: u32,
) -> CallGraphResponse {
    let g = load_call_graph(ctx);
    let depth = depth.max(1);
    let want_callers = matches!(direction, "callers" | "both" | "");
    let want_callees = matches!(direction, "callees" | "both" | "");

    let callers = if want_callers {
        g.callers(symbol, file_hint, depth)
            .into_iter()
            .map(to_ref)
            .collect()
    } else {
        vec![]
    };
    let callees = if want_callees {
        g.callees(symbol, file_hint, depth)
            .into_iter()
            .map(to_ref)
            .collect()
    } else {
        vec![]
    };

    CallGraphResponse {
        symbol: symbol.to_string(),
        callers,
        callees,
        depth,
        edge_count: g.edge_count(),
    }
}

fn to_ref(n: CallNode) -> CallRef {
    CallRef {
        file: n.file,
        symbol: n.symbol,
    }
}

pub fn affected_functions(ctx: &RepoQueryCtx, symbol: &str, depth: u32) -> Vec<CallRef> {
    let g = load_call_graph(ctx);
    g.callers(symbol, None, depth.max(1))
        .into_iter()
        .map(to_ref)
        .collect()
}

pub fn call_fan_in(ctx: &RepoQueryCtx, symbol: &str) -> usize {
    load_call_graph(ctx).fan_in(symbol)
}

#[allow(dead_code)]
pub fn call_store(ctx: &RepoQueryCtx) -> CallGraphStore {
    load_calls(&codeindex_dir(&ctx.root))
}
