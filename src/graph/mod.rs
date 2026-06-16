use petgraph::graph::DiGraph;
use petgraph::prelude::NodeIndex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub mod bfs;
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
