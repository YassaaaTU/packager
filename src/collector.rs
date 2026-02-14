//! File collector module for gathering files to package
//!
//! This module provides:
//! - Git-aware file listing (using `git ls-files` when available)
//! - Fallback to filesystem walking with gitignore semantics
//! - Parallel file collection using rayon
//! - Empty directory detection

use anyhow::{Context, Result};
use ignore::{Walk, WalkBuilder};
use std::collections::{HashSet, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::excludes::ExcludeMatcher;

/// Collected files and directories for packaging
#[derive(Debug, Default)]
pub struct FileCollection {
    /// Files to include in the archive
    pub files: BTreeSet<PathBuf>,
    /// Empty directories to include in the archive
    pub empty_dirs: BTreeSet<PathBuf>,
    /// Root directory of the repository
    pub root: PathBuf,
}

impl FileCollection {
    /// Create a new empty file collection
    pub fn new(root: PathBuf) -> Self {
        Self {
            files: BTreeSet::new(),
            empty_dirs: BTreeSet::new(),
            root,
        }
    }

    /// Check if the collection is empty
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.empty_dirs.is_empty()
    }

    /// Get the total count of files
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Get the total count of empty directories
    pub fn empty_dir_count(&self) -> usize {
        self.empty_dirs.len()
    }
}

/// File collector for gathering repository files
pub struct FileCollector {
    /// Root directory to collect from
    root: PathBuf,
    /// Whether to only include git-tracked files
    tracked_only: bool,
    /// Whether to include submodules
    recurse_submodules: bool,
    /// Exclude matcher
    exclude_matcher: ExcludeMatcher,
    /// Whether to include empty directories
    include_empty_dirs: bool,
}

impl FileCollector {
    /// Create a new file collector
    pub fn new(
        root: PathBuf,
        tracked_only: bool,
        recurse_submodules: bool,
        exclude_matcher: ExcludeMatcher,
        include_empty_dirs: bool,
    ) -> Self {
        Self {
            root,
            tracked_only,
            recurse_submodules,
            exclude_matcher,
            include_empty_dirs,
        }
    }

    /// Collect files from the repository
    pub fn collect(self) -> Result<FileCollection> {
        // Try git-based collection first
        let git_result = if self.tracked_only {
            self.collect_git_tracked()
        } else {
            self.collect_git_all()
        };

        // Use git result if successful, otherwise fall back to filesystem walk
        let mut collection = match git_result {
            Ok(mut col) => {
                // Apply user excludes
                self.apply_excludes(&mut col);
                col
            }
            Err(e) => {
                tracing::debug!("Git collection failed, falling back to filesystem walk: {}", e);
                let mut col = self.collect_filesystem()?;
                self.apply_excludes(&mut col);
                col
            }
        };

        // Find empty directories if requested
        if self.include_empty_dirs {
            self.find_empty_dirs(&mut collection)?;
        }

        Ok(collection)
    }

    /// Collect files using git ls-files (tracked files only)
    fn collect_git_tracked(&self) -> Result<FileCollection> {
        let output = Command::new("git")
            .args(["ls-files", "-z"])
            .current_dir(&self.root)
            .output()
            .context("Failed to execute git ls-files")?;

        if !output.status.success() {
            anyhow::bail!("git ls-files failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        let files: BTreeSet<PathBuf> = output
            .stdout
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| PathBuf::from(String::from_utf8_lossy(s).into_owned()))
            .collect();

        Ok(FileCollection {
            files,
            empty_dirs: BTreeSet::new(),
            root: self.root.clone(),
        })
    }

    /// Collect files using git ls-files (all files, including untracked)
    fn collect_git_all(&self) -> Result<FileCollection> {
        let mut args = vec!["ls-files", "-co", "--exclude-standard", "-z"];
        
        if self.recurse_submodules {
            args.push("--recurse-submodules");
        }

        let output = Command::new("git")
            .args(&args)
            .current_dir(&self.root)
            .output()
            .context("Failed to execute git ls-files")?;

        if !output.status.success() {
            anyhow::bail!("git ls-files failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        let files: BTreeSet<PathBuf> = output
            .stdout
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| PathBuf::from(String::from_utf8_lossy(s).into_owned()))
            .collect();

        Ok(FileCollection {
            files,
            empty_dirs: BTreeSet::new(),
            root: self.root.clone(),
        })
    }

    /// Collect files using filesystem walk with gitignore semantics
    fn collect_filesystem(&self) -> Result<FileCollection> {
        let mut files = BTreeSet::new();

        let walker = self.build_walker();
        
        for result in walker {
            match result {
                Ok(entry) => {
                    let path = entry.path();
                    if path.is_file() {
                        let relative = path.strip_prefix(&self.root)
                            .context("Failed to strip prefix from path")?;
                        files.insert(relative.to_path_buf());
                    }
                }
                Err(err) => {
                    tracing::warn!("Error walking filesystem: {}", err);
                }
            }
        }

        Ok(FileCollection {
            files,
            empty_dirs: BTreeSet::new(),
            root: self.root.clone(),
        })
    }

    /// Build a filesystem walker with gitignore semantics
    fn build_walker(&self) -> Walk {
        WalkBuilder::new(&self.root)
            .hidden(false)           // Include hidden files
            .ignore(true)            // Use .gitignore
            .git_global(true)        // Use global gitignore
            .git_exclude(true)       // Use .git/info/exclude
            .git_ignore(true)        // Use .gitignore
            .follow_links(false)     // Don't follow symlinks
            .same_file_system(true)  // Stay on same filesystem
            .build()
    }

    /// Apply user excludes to the file collection
    fn apply_excludes(&self, collection: &mut FileCollection) {
        if self.exclude_matcher.is_empty() {
            return;
        }

        collection.files.retain(|path| {
            !self.exclude_matcher.is_excluded(path)
        });
    }

    /// Find empty directories in the repository
    fn find_empty_dirs(&self, collection: &mut FileCollection) -> Result<()> {
        // Collect all directories that have files
        let mut dirs_with_files: HashSet<PathBuf> = HashSet::new();
        
        for file in &collection.files {
            let mut current = file.parent();
            while let Some(dir) = current {
                if !dir.as_os_str().is_empty() {
                    dirs_with_files.insert(dir.to_path_buf());
                }
                current = dir.parent();
            }
        }

        // Walk the filesystem to find all directories
        let walker = WalkBuilder::new(&self.root)
            .hidden(false)
            .ignore(true)
            .git_global(true)
            .git_exclude(true)
            .git_ignore(true)
            .follow_links(false)
            .build();

        let mut all_dirs: BTreeSet<PathBuf> = BTreeSet::new();
        
        for result in walker {
            match result {
                Ok(entry) => {
                    let path = entry.path();
                    if path.is_dir() {
                        let relative = path.strip_prefix(&self.root)
                            .context("Failed to strip prefix from path")?;
                        if !relative.as_os_str().is_empty() {
                            all_dirs.insert(relative.to_path_buf());
                        }
                    }
                }
                Err(err) => {
                    tracing::warn!("Error walking filesystem for directories: {}", err);
                }
            }
        }

        // Find empty directories (directories not in dirs_with_files)
        for dir in all_dirs {
            if !dirs_with_files.contains(&dir) && !self.exclude_matcher.is_excluded(&dir) {
                collection.empty_dirs.insert(dir);
            }
        }

        Ok(())
    }
}

/// Check if git is available
pub fn is_git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if a directory is a git repository
pub fn is_git_repo<P: AsRef<Path>>(path: P) -> bool {
    let git_dir = path.as_ref().join(".git");
    git_dir.exists()
}

/// Find the git root directory from a given path
pub fn find_git_root<P: AsRef<Path>>(path: P) -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(path.as_ref())
        .output()
        .ok()?;

    if output.status.success() {
        let root = String::from_utf8_lossy(&output.stdout);
        Some(PathBuf::from(root.trim()))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_file_collection_empty() {
        let collection = FileCollection::new(PathBuf::from("/tmp"));
        assert!(collection.is_empty());
        assert_eq!(collection.file_count(), 0);
        assert_eq!(collection.empty_dir_count(), 0);
    }

    #[test]
    fn test_file_collection_with_files() {
        let mut collection = FileCollection::new(PathBuf::from("/tmp"));
        collection.files.insert(PathBuf::from("src/main.rs"));
        collection.files.insert(PathBuf::from("Cargo.toml"));

        assert!(!collection.is_empty());
        assert_eq!(collection.file_count(), 2);
    }

    #[test]
    fn test_is_git_available() {
        // This test depends on the system having git installed
        // We just verify the function doesn't panic
        let _ = is_git_available();
    }

    #[test]
    fn test_is_git_repo() {
        let temp_dir = TempDir::new().unwrap();
        assert!(!is_git_repo(temp_dir.path()));
    }

    #[test]
    fn test_collector_filesystem() {
        let temp_dir = TempDir::new().unwrap();
        
        // Create some files
        fs::create_dir_all(temp_dir.path().join("src")).unwrap();
        fs::write(temp_dir.path().join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(temp_dir.path().join("Cargo.toml"), "[package]").unwrap();

        let exclude_matcher = ExcludeMatcher::new(vec![]).unwrap();
        let collector = FileCollector::new(
            temp_dir.path().to_path_buf(),
            false,
            false,
            exclude_matcher,
            false,
        );

        let collection = collector.collect().unwrap();
        
        assert!(collection.file_count() >= 2);
        assert!(collection.files.contains(&PathBuf::from("src/main.rs")));
        assert!(collection.files.contains(&PathBuf::from("Cargo.toml")));
    }

    #[test]
    fn test_collector_with_excludes() {
        let temp_dir = TempDir::new().unwrap();
        
        // Create some files
        fs::create_dir_all(temp_dir.path().join("src")).unwrap();
        fs::create_dir_all(temp_dir.path().join("target")).unwrap();
        fs::write(temp_dir.path().join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(temp_dir.path().join("target/debug.bin"), "binary").unwrap();

        let exclude_matcher = ExcludeMatcher::new(vec!["target".to_string()]).unwrap();
        let collector = FileCollector::new(
            temp_dir.path().to_path_buf(),
            false,
            false,
            exclude_matcher,
            false,
        );

        let collection = collector.collect().unwrap();
        
        assert!(collection.files.contains(&PathBuf::from("src/main.rs")));
        assert!(!collection.files.contains(&PathBuf::from("target/debug.bin")));
    }

    #[test]
    fn test_collector_empty_dirs() {
        let temp_dir = TempDir::new().unwrap();
        
        // Create files and an empty directory
        fs::create_dir_all(temp_dir.path().join("src")).unwrap();
        fs::create_dir_all(temp_dir.path().join("empty")).unwrap();
        fs::write(temp_dir.path().join("src/main.rs"), "fn main() {}").unwrap();

        let exclude_matcher = ExcludeMatcher::new(vec![]).unwrap();
        let collector = FileCollector::new(
            temp_dir.path().to_path_buf(),
            false,
            false,
            exclude_matcher,
            true,  // include empty dirs
        );

        let collection = collector.collect().unwrap();
        
        assert!(collection.empty_dirs.contains(&PathBuf::from("empty")));
    }
}
