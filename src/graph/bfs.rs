use crate::graph::DependencyGraph;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

pub fn score_files(graph: &DependencyGraph, active_files: &[PathBuf]) -> HashMap<PathBuf, f32> {
    let mut scores: HashMap<PathBuf, f32> = HashMap::new();
    let mut queue: VecDeque<(PathBuf, u32)> = VecDeque::new();

    for f in active_files {
        scores.insert(f.clone(), 1.0);
        queue.push_back((f.clone(), 0));
    }

    while let Some((file, depth)) = queue.pop_front() {
        let next_depth = depth + 1;
        let next_score = hop_score(next_depth);

        for neighbor in graph.neighbors(&file) {
            let entry = scores.entry(neighbor.clone()).or_insert(0.0);
            if next_score > *entry {
                *entry = next_score;
                queue.push_back((neighbor, next_depth));
            }
        }
    }

    for file in graph.all_files() {
        scores.entry(file).or_insert(0.1);
    }

    scores
}

fn hop_score(depth: u32) -> f32 {
    match depth {
        0 => 1.0,
        1 => 0.5,
        2 => 0.25,
        _ => 0.1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::DependencyGraph;
    use std::path::PathBuf;

    fn make_graph() -> DependencyGraph {
        let mut g = DependencyGraph::new();
        // auth → db → pool
        g.add_dependency(&PathBuf::from("auth.rs"), &PathBuf::from("db.rs"));
        g.add_dependency(&PathBuf::from("db.rs"), &PathBuf::from("pool.rs"));
        g.add_dependency(&PathBuf::from("auth.rs"), &PathBuf::from("jwt.rs"));
        g
    }

    #[test]
    fn active_file_scores_1_0() {
        let g = make_graph();
        let scores = score_files(&g, &[PathBuf::from("auth.rs")]);
        assert!((scores[&PathBuf::from("auth.rs")] - 1.0).abs() < 0.001);
    }

    #[test]
    fn one_hop_scores_0_5() {
        let g = make_graph();
        let scores = score_files(&g, &[PathBuf::from("auth.rs")]);
        assert!((scores[&PathBuf::from("db.rs")] - 0.5).abs() < 0.001);
        assert!((scores[&PathBuf::from("jwt.rs")] - 0.5).abs() < 0.001);
    }

    #[test]
    fn two_hops_scores_0_25() {
        let g = make_graph();
        let scores = score_files(&g, &[PathBuf::from("auth.rs")]);
        assert!((scores[&PathBuf::from("pool.rs")] - 0.25).abs() < 0.001);
    }

    #[test]
    fn unrelated_file_scores_0_1() {
        let mut g = make_graph();
        g.add_dependency(&PathBuf::from("unrelated.rs"), &PathBuf::from("other.rs"));
        let scores = score_files(&g, &[PathBuf::from("auth.rs")]);
        assert!((scores[&PathBuf::from("unrelated.rs")] - 0.1).abs() < 0.001);
    }
}
