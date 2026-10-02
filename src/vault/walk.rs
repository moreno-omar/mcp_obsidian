//! Which files a vault scan includes.
//!
//! Kept separate from [`super::index`] so the traversal rules (hidden dirs,
//! symlinks, markdown detection) read as one small policy instead of being
//! interleaved with index building.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use walkdir::WalkDir;

/// Directories that never contain user notes and are pruned during the scan.
const SKIPPED_DIRS: [&str; 7] = [
    ".obsidian",
    ".git",
    ".trash",
    ".stversions",
    ".venv",
    "node_modules",
    "target",
];

/// Every markdown file under `root`, sorted by path for reproducible indexing.
///
/// Symlinks are not followed (a cycle would otherwise loop forever) and any
/// directory starting with `.` is pruned. The sort guarantees that note order —
/// and therefore every rendered result — does not depend on filesystem order.
pub(crate) fn markdown_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| !is_skipped_dir(entry))
    {
        let entry = entry.with_context(|| format!("failed to scan {}", root.display()))?;
        if entry.file_type().is_file() && is_markdown(entry.path()) {
            files.push(entry.path().to_path_buf());
        }
    }
    files.sort();
    Ok(files)
}

/// Read a markdown file, reporting which path failed. UTF-8 is required.
pub(crate) fn read_note(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
}

/// `true` for files with a `.md` extension, any case.
fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("md"))
        .unwrap_or(false)
}

/// `true` for dot-directories and the known non-note directories.
fn is_skipped_dir(entry: &walkdir::DirEntry) -> bool {
    if entry.depth() == 0 || !entry.file_type().is_dir() {
        return false;
    }
    entry
        .file_name()
        .to_str()
        .map(|name| name.starts_with('.') || SKIPPED_DIRS.contains(&name))
        .unwrap_or(false)
}
