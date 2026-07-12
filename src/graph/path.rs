use crate::graph::DependencyGraph;
use std::path::PathBuf;

/// Shortest dependency path from `from` to `to` (follows import edges).
/// Returns `None` if either node is missing or no path exists.
pub fn shortest_path(
    graph: &DependencyGraph,
    from: &PathBuf,
    to: &PathBuf,
) -> Option<Vec<PathBuf>> {
    graph.shortest_path(from, to)
}

/// Top files by reverse-dependency count (god nodes / hotspots).
pub fn hotspots(graph: &DependencyGraph, limit: usize) -> Vec<(PathBuf, usize)> {
    graph.hotspots(limit)
}

/// Format a path as Graphify-style hop list.
pub fn format_path_hops(path: &[PathBuf]) -> String {
    if path.is_empty() {
        return String::new();
    }
    if path.len() == 1 {
        return path[0].display().to_string();
    }
    path.windows(2)
        .map(|w| w[0].display().to_string())
        .chain(std::iter::once(path.last().unwrap().display().to_string()))
        .collect::<Vec<_>>()
        .join(" --imports--> ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain_graph() -> DependencyGraph {
        let mut g = DependencyGraph::new();
        let a = PathBuf::from("src/a.rs");
        let b = PathBuf::from("src/b.rs");
        let c = PathBuf::from("src/c.rs");
        g.add_dependency(&a, &b);
        g.add_dependency(&b, &c);
        g
    }

    #[test]
    fn shortest_path_finds_chain() {
        let g = chain_graph();
        let path = shortest_path(
            &g,
            &PathBuf::from("src/a.rs"),
            &PathBuf::from("src/c.rs"),
        )
        .unwrap();
        assert_eq!(path.len(), 3);
        assert_eq!(path[0], PathBuf::from("src/a.rs"));
        assert_eq!(path[2], PathBuf::from("src/c.rs"));
    }

    #[test]
    fn shortest_path_none_when_disconnected() {
        let mut g = DependencyGraph::new();
        g.add_dependency(
            &PathBuf::from("src/x.rs"),
            &PathBuf::from("src/y.rs"),
        );
        assert!(shortest_path(
            &g,
            &PathBuf::from("src/x.rs"),
            &PathBuf::from("src/z.rs"),
        )
        .is_none());
    }

    #[test]
    fn format_path_hops_graphify_style() {
        let path = vec![
            PathBuf::from("auth.rs"),
            PathBuf::from("db.rs"),
            PathBuf::from("pool.rs"),
        ];
        let s = format_path_hops(&path);
        assert_eq!(s, "auth.rs --imports--> db.rs --imports--> pool.rs");
    }

    #[test]
    fn hotspots_ranks_by_dependent_count() {
        let mut g = DependencyGraph::new();
        let lib = PathBuf::from("src/lib.rs");
        let auth = PathBuf::from("src/auth.rs");
        let db = PathBuf::from("src/db.rs");
        g.add_dependency(&lib, &auth);
        g.add_dependency(&lib, &db);
        g.add_dependency(&auth, &db);
        let hs = hotspots(&g, 10);
        assert!(!hs.is_empty());
        assert_eq!(hs[0].0, db);
        assert_eq!(hs[0].1, 2);
    }
}
