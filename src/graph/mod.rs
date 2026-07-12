use petgraph::algo::astar;
use petgraph::graph::DiGraph;
use petgraph::prelude::NodeIndex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub mod bfs;
pub mod path;
pub mod persistence;

#[derive(Debug, Serialize, Deserialize)]
pub struct DependencyGraph {
    graph: DiGraph<PathBuf, ()>,
    node_map: HashMap<PathBuf, NodeIndex>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_map: HashMap::new(),
        }
    }

    fn get_or_insert(&mut self, path: &PathBuf) -> NodeIndex {
        if let Some(&idx) = self.node_map.get(path) {
            return idx;
        }
        let idx = self.graph.add_node(path.clone());
        self.node_map.insert(path.clone(), idx);
        idx
    }

    pub fn add_dependency(&mut self, from: &PathBuf, to: &PathBuf) {
        let a = self.get_or_insert(from);
        let b = self.get_or_insert(to);
        if !self.graph.contains_edge(a, b) {
            self.graph.add_edge(a, b, ());
        }
    }

    pub fn has_dependency(&self, from: &PathBuf, to: &PathBuf) -> bool {
        let a = match self.node_map.get(from) {
            Some(&i) => i,
            None => return false,
        };
        let b = match self.node_map.get(to) {
            Some(&i) => i,
            None => return false,
        };
        self.graph.contains_edge(a, b)
    }

    pub fn remove_file(&mut self, path: &PathBuf) {
        if let Some(&idx) = self.node_map.get(path) {
            self.graph.remove_node(idx);
            self.node_map.remove(path);
        }
    }

    pub fn neighbors(&self, path: &PathBuf) -> Vec<PathBuf> {
        let idx = match self.node_map.get(path) {
            Some(&i) => i,
            None => return vec![],
        };
        self.graph
            .neighbors(idx)
            .map(|n| self.graph[n].clone())
            .collect()
    }

    pub fn reverse_neighbors(&self, path: &PathBuf) -> Vec<PathBuf> {
        use petgraph::Direction;
        let idx = match self.node_map.get(path) {
            Some(&i) => i,
            None => return vec![],
        };
        self.graph
            .neighbors_directed(idx, Direction::Incoming)
            .map(|n| self.graph[n].clone())
            .collect()
    }

    pub fn all_files(&self) -> Vec<PathBuf> {
        self.node_map.keys().cloned().collect()
    }

    /// Resolve a path string to a node in the graph (exact or suffix match).
    pub fn resolve_node(&self, hint: &str) -> Option<PathBuf> {
        let hint_path = PathBuf::from(hint);
        if self.node_map.contains_key(&hint_path) {
            return Some(hint_path);
        }
        let hint_norm = hint.replace('\\', "/");
        self.node_map
            .keys()
            .find(|p| {
                let s = p.to_string_lossy().replace('\\', "/");
                s == hint_norm || s.ends_with(&hint_norm) || hint_norm.ends_with(&s)
            })
            .cloned()
    }

    /// Shortest directed path following import edges (`from` imports … eventually `to`).
    pub fn shortest_path(&self, from: &PathBuf, to: &PathBuf) -> Option<Vec<PathBuf>> {
        let from_idx = self.resolve_node(&from.to_string_lossy())?;
        let to_idx = self.resolve_node(&to.to_string_lossy())?;
        let from_idx = *self.node_map.get(&from_idx)?;
        let to_idx = *self.node_map.get(&to_idx)?;

        let result = astar(
            &self.graph,
            from_idx,
            |n| n == to_idx,
            |_| 1u32,
            |_| 0u32,
        )?;
        Some(result.1.into_iter().map(|n| self.graph[n].clone()).collect())
    }

    /// Top files by number of direct dependents (reverse edge count).
    pub fn hotspots(&self, limit: usize) -> Vec<(PathBuf, usize)> {
        use petgraph::Direction;
        let mut counts: HashMap<PathBuf, usize> = HashMap::new();
        for path in self.node_map.keys() {
            let idx = self.node_map[path];
            let n = self
                .graph
                .neighbors_directed(idx, Direction::Incoming)
                .count();
            counts.insert(path.clone(), n);
        }
        let mut ranked: Vec<_> = counts.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        ranked.truncate(limit);
        ranked
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    pub fn node_count(&self) -> usize {
        self.node_map.len()
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn add_dependency_creates_edge() {
        let mut g = DependencyGraph::new();
        let a = PathBuf::from("src/auth.rs");
        let b = PathBuf::from("src/db.rs");
        g.add_dependency(&a, &b);
        assert!(g.has_dependency(&a, &b));
    }

    #[test]
    fn remove_file_removes_all_edges() {
        let mut g = DependencyGraph::new();
        let a = PathBuf::from("src/auth.rs");
        let b = PathBuf::from("src/db.rs");
        g.add_dependency(&a, &b);
        g.remove_file(&a);
        assert!(!g.has_dependency(&a, &b));
    }

    #[test]
    fn neighbors_returns_direct_dependencies() {
        let mut g = DependencyGraph::new();
        let a = PathBuf::from("src/auth.rs");
        let b = PathBuf::from("src/db.rs");
        let c = PathBuf::from("src/jwt.rs");
        g.add_dependency(&a, &b);
        g.add_dependency(&a, &c);
        let neighbors = g.neighbors(&a);
        assert!(neighbors.contains(&b));
        assert!(neighbors.contains(&c));
    }

    #[test]
    fn reverse_neighbors_returns_dependents() {
        let mut g = DependencyGraph::new();
        let a = PathBuf::from("src/auth.rs");
        let b = PathBuf::from("src/db.rs");
        g.add_dependency(&a, &b);
        let dependents = g.reverse_neighbors(&b);
        assert!(dependents.contains(&a));
    }
}
