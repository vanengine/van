use crate::config::VanConfig;
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A loaded Van project, providing file collection and mock data utilities.
#[derive(Clone)]
pub struct VanProject {
    pub root: PathBuf,
    pub config: VanConfig,
}

impl VanProject {
    /// Load a Van project from the given directory.
    pub fn load(dir: &Path) -> Result<Self> {
        let pkg_path = dir.join("package.json");
        if !pkg_path.exists() {
            bail!("No package.json found. Are you in a Van project directory?");
        }
        let pkg_raw =
            fs::read_to_string(&pkg_path).context("Failed to read package.json")?;
        let config: VanConfig =
            serde_json::from_str(&pkg_raw).context("Failed to parse package.json")?;
        Ok(Self {
            root: dir.to_path_buf(),
            config,
        })
    }

    /// Load a Van project from the current working directory.
    pub fn load_cwd() -> Result<Self> {
        let cwd = std::env::current_dir()?;
        Self::load(&cwd)
    }

    /// Collect all source files (.van, .ts, .js) from `src/` and `node_modules/@scope/`.
    ///
    /// Returns a HashMap keyed by relative path (e.g. `"pages/index.van"`).
    pub fn collect_files(&self) -> Result<HashMap<String, String>> {
        let src_dir = self.src_dir();
        if !src_dir.exists() {
            bail!("No src/ directory found.");
        }
        let mut files = HashMap::new();
        collect_files_recursive(&src_dir, &src_dir, &mut files)?;

        let node_modules = self.root.join("node_modules");
        if node_modules.exists() {
            collect_node_modules(&node_modules, &mut files)?;
        }

        Ok(files)
    }

    /// Load page-specific mock data from `mock/index.json`.
    ///
    /// Tries page-specific key first (e.g. `"pages/index"`), falls back to root object.
    pub fn load_mock_data(&self, page_key: &str) -> Value {
        let all = self.load_all_mock_data();
        if let Some(page_data) = all.get(page_key) {
            page_data.clone()
        } else {
            all
        }
    }

    /// Load all mock data from `mock/index.json`.
    pub fn load_all_mock_data(&self) -> Value {
        let mock_path = self.root.join("mock/index.json");
        let content = match fs::read_to_string(&mock_path) {
            Ok(c) => c,
            Err(_) => return Value::Object(Default::default()),
        };
        match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => Value::Object(Default::default()),
        }
    }

    /// Find all page entries (files under `pages/` with `.van` extension).
    pub fn page_entries(&self, files: &HashMap<String, String>) -> Vec<String> {
        find_van_files(files, "pages/")
    }

    /// Find all component entries (files under `components/` with `.van` extension).
    pub fn component_entries(&self, files: &HashMap<String, String>) -> Vec<String> {
        find_van_files(files, "components/")
    }

    pub fn src_dir(&self) -> PathBuf {
        self.root.join("src")
    }

    pub fn pages_dir(&self) -> PathBuf {
        self.root.join("src").join("pages")
    }

    pub fn dist_dir(&self) -> PathBuf {
        self.root.join("dist")
    }
}

/// Recursively collect source files (.van, .ts, .js) into the map.
/// Keys are relative to `base` (e.g. `pages/index.van`).
fn collect_files_recursive(
    dir: &Path,
    base: &Path,
    files: &mut HashMap<String, String>,
) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(&path, base, files)?;
        } else if is_source_file(&path) {
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            files.insert(rel, content);
        }
    }
    Ok(())
}

/// Collect scoped package files from `node_modules/@scope/` directories.
/// Keys are like `@van-ui/button/button.van`.
fn collect_node_modules(
    node_modules: &Path,
    files: &mut HashMap<String, String>,
) -> Result<()> {
    for scope_entry in fs::read_dir(node_modules)? {
        let scope_entry = scope_entry?;
        let scope_name = scope_entry.file_name().to_string_lossy().to_string();
        if !scope_name.starts_with('@') || !scope_entry.path().is_dir() {
            continue;
        }
        for pkg_entry in fs::read_dir(scope_entry.path())? {
            let pkg_entry = pkg_entry?;
            if !pkg_entry.path().is_dir() {
                continue;
            }
            let pkg_dir = pkg_entry.path();
            let pkg_name = pkg_dir.file_name().unwrap().to_string_lossy().to_string();
            collect_scoped_package_recursive(&pkg_dir, &scope_name, &pkg_name, &pkg_dir, files)?;
        }
    }
    Ok(())
}

fn collect_scoped_package_recursive(
    dir: &Path,
    scope_name: &str,
    pkg_name: &str,
    pkg_dir: &Path,
    files: &mut HashMap<String, String>,
) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_scoped_package_recursive(&path, scope_name, pkg_name, pkg_dir, files)?;
        } else if is_source_file(&path) {
            let rel = path
                .strip_prefix(pkg_dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let key = format!("{}/{}/{}", scope_name, pkg_name, rel);
            let content = fs::read_to_string(&path)?;
            files.insert(key, content);
        }
    }
    Ok(())
}

fn is_source_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("van" | "ts" | "js")
    )
}

fn find_van_files(files: &HashMap<String, String>, prefix: &str) -> Vec<String> {
    let mut entries: Vec<String> = files
        .keys()
        .filter(|k| k.starts_with(prefix) && k.ends_with(".van"))
        .cloned()
        .collect();
    entries.sort();
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_source_file() {
        assert!(is_source_file(Path::new("foo.van")));
        assert!(is_source_file(Path::new("bar.ts")));
        assert!(is_source_file(Path::new("baz.js")));
        assert!(!is_source_file(Path::new("readme.md")));
        assert!(!is_source_file(Path::new("style.css")));
    }

    #[test]
    fn test_find_van_files() {
        let mut files = HashMap::new();
        files.insert("pages/index.van".into(), String::new());
        files.insert("pages/about.van".into(), String::new());
        files.insert("components/header.van".into(), String::new());
        files.insert("utils/format.ts".into(), String::new());

        let pages = find_van_files(&files, "pages/");
        assert_eq!(pages, vec!["pages/about.van", "pages/index.van"]);

        let components = find_van_files(&files, "components/");
        assert_eq!(components, vec!["components/header.van"]);
    }
}
