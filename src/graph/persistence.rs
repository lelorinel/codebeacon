use crate::graph::DependencyGraph;
use anyhow::Result;
use std::path::Path;

pub fn save(graph: &DependencyGraph, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = bincode::serialize(graph)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

pub fn load(path: &Path) -> Result<DependencyGraph> {
    let bytes = std::fs::read(path)?;
    let graph = bincode::deserialize(&bytes)?;
    Ok(graph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::DependencyGraph;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn round_trip_preserves_edges() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("graph.bin");

        let mut g = DependencyGraph::new();
        g.add_dependency(&PathBuf::from("a.rs"), &PathBuf::from("b.rs"));
        g.add_dependency(&PathBuf::from("b.rs"), &PathBuf::from("c.rs"));

        save(&g, &path).unwrap();
        let loaded = load(&path).unwrap();

        assert!(loaded.has_dependency(&PathBuf::from("a.rs"), &PathBuf::from("b.rs")));
        assert!(loaded.has_dependency(&PathBuf::from("b.rs"), &PathBuf::from("c.rs")));
        assert!(!loaded.has_dependency(&PathBuf::from("a.rs"), &PathBuf::from("c.rs")));
    }
}
