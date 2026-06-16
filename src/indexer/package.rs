use crate::types::{FileEntry, PackageDetail};
use std::collections::HashMap;

pub fn group_into_packages(files: Vec<FileEntry>) -> Vec<PackageDetail> {
    let mut groups: HashMap<String, Vec<FileEntry>> = HashMap::new();

    for file in files {
        let pkg_name = package_name_for(&file.path);
        groups.entry(pkg_name).or_default().push(file);
    }

    groups.into_iter().map(|(name, files)| PackageDetail {
        name,
        files,
    }).collect()
}

fn package_name_for(path: &std::path::Path) -> String {
    // Collect only directory components (no dots = not a file)
    let dirs: Vec<String> = path.components()
        .filter_map(|c| {
            let s = c.as_os_str().to_string_lossy();
            if s.contains('.') { None } else { Some(s.into_owned()) }
        })
        .collect();

    match dirs.as_slice() {
        [] => "root".to_string(),
        [only] => only.clone(),                               // "auth/login.rs" → "auth"
        [first, second, ..] if first == "src" => second.clone(), // "src/auth/login.rs" → "auth"
        [first, ..] => first.clone(),                        // "cmd/main.go" → "cmd"
    }
}

pub fn hot_symbols(packages: &[PackageDetail], limit: usize) -> Vec<String> {
    let mut symbols: Vec<String> = packages.iter()
        .flat_map(|p| p.files.iter())
        .flat_map(|f| f.symbols.iter().map(|s| s.name.clone()))
        .collect();
    symbols.sort();
    symbols.dedup();
    symbols.truncate(limit);
    symbols
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::path::PathBuf;

    fn file(path: &str) -> FileEntry {
        FileEntry {
            path: PathBuf::from(path),
            symbols: vec![],
            depends_on: vec![],
            depended_by: vec![],
        }
    }

    #[test]
    fn groups_files_by_first_directory() {
        let files = vec![
            file("src/auth/login.rs"),
            file("src/auth/logout.rs"),
            file("src/db/pool.rs"),
        ];
        let packages = group_into_packages(files);
        assert_eq!(packages.len(), 2);
        let auth = packages.iter().find(|p| p.name == "auth").unwrap();
        assert_eq!(auth.files.len(), 2);
        let db = packages.iter().find(|p| p.name == "db").unwrap();
        assert_eq!(db.files.len(), 1);
    }

    #[test]
    fn root_level_files_go_to_root_package() {
        let files = vec![file("main.rs"), file("lib.rs")];
        let packages = group_into_packages(files);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "root");
    }
}
