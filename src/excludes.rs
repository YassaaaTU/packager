//! Exclude matching module for handling user-specified exclude patterns
//!
//! This module provides:
//! - Pattern normalization (path prefix and glob matching)
//! - Efficient matching using globset
//! - Support for directory and file excludes

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::{Path, PathBuf};

/// Matcher for exclude patterns
pub struct ExcludeMatcher {
    /// Prefix patterns for directory/file matching
    prefix_patterns: Vec<String>,
    /// Glob patterns for wildcard matching
    glob_set: GlobSet,
    /// Original patterns for debugging
    patterns: Vec<String>,
}

impl ExcludeMatcher {
    /// Create a new exclude matcher from a list of patterns
    pub fn new(patterns: Vec<String>) -> Result<Self> {
        let mut prefix_patterns = Vec::new();
        let mut glob_builder = GlobSetBuilder::new();

        for pattern in &patterns {
            let normalized = normalize_pattern(pattern);

            // Check if pattern is a simple prefix (no wildcards)
            if is_prefix_pattern(&normalized) {
                prefix_patterns.push(normalized);
            } else {
                // Add as glob pattern
                let glob = Glob::new(&normalized)
                    .with_context(|| format!("Invalid glob pattern: {}", pattern))?;
                glob_builder.add(glob);
            }
        }

        let glob_set = glob_builder.build()
            .context("Failed to build glob set")?;

        Ok(Self {
            prefix_patterns,
            glob_set,
            patterns,
        })
    }

    /// Check if a path should be excluded
    pub fn is_excluded<P: AsRef<Path>>(&self, path: P) -> bool {
        let path = path.as_ref();
        let normalized = normalize_path_string(path);

        // Check prefix patterns
        for prefix in &self.prefix_patterns {
            if normalized.starts_with(prefix) || normalized == prefix.trim_end_matches('/') {
                return true;
            }
            // Also check if the path is inside a prefix directory
            if prefix.ends_with('/') && normalized.starts_with(prefix) {
                return true;
            }
        }

        // Check glob patterns
        if self.glob_set.is_match(&normalized) {
            return true;
        }

        // Also check with the path as-is for glob matching
        if self.glob_set.is_match(path) {
            return true;
        }

        false
    }

    /// Get the list of patterns
    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }

    /// Check if there are any patterns
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }
}

/// Normalize a pattern for consistent matching
fn normalize_pattern(pattern: &str) -> String {
    // Convert backslashes to forward slashes
    let mut normalized = pattern.replace('\\', "/");

    // Remove leading ./ if present
    if normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }

    // Ensure trailing slash for directory patterns
    // (already handled by the user in most cases)

    normalized
}

/// Check if a pattern is a simple prefix pattern (no wildcards)
fn is_prefix_pattern(pattern: &str) -> bool {
    !pattern.contains('*') 
        && !pattern.contains('?') 
        && !pattern.contains('[') 
        && !pattern.contains('{')
}

/// Normalize a path to a forward-slash string
fn normalize_path_string(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}

/// Normalize a path for consistent comparison
pub fn normalize_path(path: &Path) -> PathBuf {
    path.components()
        .filter(|c| c.as_os_str() != ".")
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_pattern_detection() {
        assert!(is_prefix_pattern("node_modules"));
        assert!(is_prefix_pattern("packages/Odoo"));
        assert!(is_prefix_pattern("target/"));
        assert!(!is_prefix_pattern("*.log"));
        assert!(!is_prefix_pattern("packages/*"));
        assert!(!is_prefix_pattern("test?"));
        assert!(!is_prefix_pattern("[abc]"));
    }

    #[test]
    fn test_pattern_normalization() {
        assert_eq!(normalize_pattern("node_modules"), "node_modules");
        assert_eq!(normalize_pattern(".\\target"), "target");
        assert_eq!(normalize_pattern("./packages/Odoo"), "packages/Odoo");
        assert_eq!(normalize_pattern("packages\\Odoo"), "packages/Odoo");
    }

    #[test]
    fn test_exclude_matcher_prefix() {
        let matcher = ExcludeMatcher::new(vec![
            "node_modules".to_string(),
            "packages/Odoo".to_string(),
            "target/".to_string(),
        ]).unwrap();

        assert!(matcher.is_excluded("node_modules"));
        assert!(matcher.is_excluded("node_modules/package.json"));
        assert!(matcher.is_excluded("packages/Odoo"));
        assert!(matcher.is_excluded("packages/Odoo/__manifest__.py"));
        assert!(matcher.is_excluded("target"));
        assert!(matcher.is_excluded("target/debug"));

        assert!(!matcher.is_excluded("src/main.rs"));
        assert!(!matcher.is_excluded("packages/Other"));
        assert!(!matcher.is_excluded("Cargo.toml"));
    }

    #[test]
    fn test_exclude_matcher_glob() {
        let matcher = ExcludeMatcher::new(vec![
            "*.log".to_string(),
            "*.tmp".to_string(),
            "docs/**/*.md".to_string(),
        ]).unwrap();

        assert!(matcher.is_excluded("debug.log"));
        assert!(matcher.is_excluded("error.log"));
        assert!(matcher.is_excluded("temp.tmp"));
        assert!(matcher.is_excluded("docs/README.md"));
        assert!(matcher.is_excluded("docs/api/guide.md"));

        assert!(!matcher.is_excluded("main.rs"));
        assert!(!matcher.is_excluded("README.md"));
        assert!(!matcher.is_excluded("src/lib.rs"));
    }

    #[test]
    fn test_exclude_matcher_mixed() {
        let matcher = ExcludeMatcher::new(vec![
            "node_modules".to_string(),
            "*.log".to_string(),
            "packages/Odoo".to_string(),
            "**/test/**".to_string(),
        ]).unwrap();

        assert!(matcher.is_excluded("node_modules"));
        assert!(matcher.is_excluded("debug.log"));
        assert!(matcher.is_excluded("packages/Odoo"));
        assert!(matcher.is_excluded("src/test/unit.rs"));
        assert!(matcher.is_excluded("test/integration/main.rs"));

        assert!(!matcher.is_excluded("src/main.rs"));
        assert!(!matcher.is_excluded("packages/Other/lib.rs"));
    }

    #[test]
    fn test_exclude_matcher_empty() {
        let matcher = ExcludeMatcher::new(vec![]).unwrap();
        
        assert!(matcher.is_empty());
        assert!(!matcher.is_excluded("anything"));
    }

    #[test]
    fn test_path_normalization() {
        assert_eq!(
            normalize_path(Path::new("./src/main.rs")),
            PathBuf::from("src/main.rs")
        );
        assert_eq!(
            normalize_path(Path::new("src/./main.rs")),
            PathBuf::from("src/main.rs")
        );
    }

    #[test]
    fn test_windows_paths() {
        // Test that Windows paths are handled correctly
        let matcher = ExcludeMatcher::new(vec![
            "packages\\Odoo".to_string(),
        ]).unwrap();

        // Both forward and backslash paths should match
        assert!(matcher.is_excluded("packages/Odoo"));
        assert!(matcher.is_excluded("packages\\Odoo"));
        assert!(matcher.is_excluded("packages/Odoo/__manifest__.py"));
    }
}
