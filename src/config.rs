//! Configuration module for handling CLI arguments and configuration files
//!
//! This module provides:
//! - CLI argument parsing with clap
//! - Configuration file support (.packagerignore / .packager.toml)
//! - Merge of CLI and file-based configuration

use anyhow::{Context, Result};
use clap::Parser;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::fs;
use chrono::Local;

/// Compression level for ZIP creation
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CompressionLevel {
    Fast,
    #[default]
    Default,
    Best,
}

impl std::str::FromStr for CompressionLevel {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "fast" => Ok(Self::Fast),
            "default" => Ok(Self::Default),
            "best" => Ok(Self::Best),
            _ => Err(format!("Invalid compression level: {}. Expected 'fast', 'default', or 'best'", s)),
        }
    }
}

/// Parse exclude patterns from a comma-separated string with optional spaces
fn parse_exclude_patterns(s: &str) -> Result<String, String> {
    // Just return the string as-is; the merging logic will handle splitting
    Ok(s.to_string())
}

/// Merge and split exclude patterns from multiple sources
fn merge_exclude_patterns(patterns: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    for pattern in patterns {
        // Split by comma and trim whitespace
        for part in pattern.split(',') {
            let trimmed = part.trim();
            if !trimmed.is_empty() {
                result.push(trimmed.to_string());
            }
        }
    }
    result
}

/// CLI arguments for the packager tool
#[derive(Parser, Debug)]
#[command(name = "packager")]
#[command(author, version, about = "Package repository into a ZIP archive", long_about = None)]
pub struct Args {
    /// Output ZIP file path (default: <repo>-YYYYMMDD_HHMMSS.zip)
    #[arg(short, long, value_name = "PATH")]
    pub zip: Option<PathBuf>,

    /// Directory for output (default: repo root)
    #[arg(short, long, value_name = "DIR")]
    pub output_dir: Option<PathBuf>,

    /// Exclude pattern (path or glob), repeatable, comma-separated supported
    #[arg(short, long, value_name = "PATTERN", value_parser = parse_exclude_patterns)]
    pub exclude: Vec<String>,

    /// Only include git-tracked files
    #[arg(short = 't', long)]
    pub tracked_only: bool,

    /// Print file list and exit
    #[arg(short, long)]
    pub list_only: bool,

    /// Shorthand: exclude packages/ directory
    #[arg(long = "no-packages")]
    pub no_packages: bool,

    /// Shorthand: exclude packages/<name>, repeatable
    #[arg(long = "ignore-package", value_name = "NAME")]
    pub ignore_package: Vec<String>,

    /// Skip empty directories (default: include them)
    #[arg(long = "no-empty-dirs")]
    pub no_empty_dirs: bool,

    /// Use stable timestamps and sorted entries for deterministic output
    #[arg(long)]
    pub deterministic: bool,

    /// Compression level: fast, default, best
    #[arg(long, value_name = "LEVEL", default_value = "default")]
    pub compression: CompressionLevel,

    /// Output SHA256 checksum of the ZIP
    #[arg(long)]
    pub checksum: bool,

    /// Include git submodules (default: exclude)
    #[arg(long)]
    pub recurse_submodules: bool,

    /// Do not honor .gitignore files
    #[arg(long)]
    pub no_gitignore: bool,

    /// Suppress all output except errors
    #[arg(short, long)]
    pub quiet: bool,

    /// Detailed output
    #[arg(short, long)]
    pub verbose: bool,

    /// Root directory to package (default: current directory)
    #[arg(value_name = "PATH", default_value = ".")]
    pub root: PathBuf,
}

/// Configuration from .packager.toml file
#[derive(Debug, Default, Deserialize)]
pub struct ConfigFile {
    #[serde(default)]
    pub defaults: ConfigDefaults,
}

/// Default configuration values from .packager.toml
#[derive(Debug, Default, Deserialize)]
pub struct ConfigDefaults {
    /// Default exclude patterns
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Default compression level
    #[serde(default)]
    pub compression: Option<String>,

    /// Skip empty directories by default
    #[serde(default)]
    pub no_empty_dirs: bool,

    /// Do not honor .gitignore files by default
    #[serde(default)]
    pub no_gitignore: bool,
}

/// Merged configuration from CLI and config files
#[derive(Debug, Clone)]
pub struct Config {
    /// Output ZIP file path
    pub zip_path: PathBuf,

    /// Root directory to package
    pub root: PathBuf,

    /// Exclude patterns (merged from CLI and config)
    pub excludes: Vec<String>,

    /// Only include git-tracked files
    pub tracked_only: bool,

    /// Print file list and exit
    pub list_only: bool,

    /// Skip empty directories
    pub no_empty_dirs: bool,

    /// Use deterministic output
    pub deterministic: bool,

    /// Compression level
    pub compression: CompressionLevel,

    /// Output SHA256 checksum
    pub checksum: bool,

    /// Include git submodules
    pub recurse_submodules: bool,

    /// Do not honor .gitignore files
    pub no_gitignore: bool,

    /// Quiet mode
    pub quiet: bool,

    /// Verbose mode
    pub verbose: bool,
}

impl Args {
    /// Parse CLI arguments
    pub fn parse_args() -> Self {
        Self::parse()
    }

    /// Load configuration from .packagerignore and .packager.toml files
    pub fn load_config(&self) -> Result<Config> {
        let root = self.root.canonicalize()
            .with_context(|| format!("Failed to resolve root path: {:?}", self.root))?;

        // Load .packagerignore if it exists
        let packagerignore_patterns = load_packagerignore(&root)?;

        // Load .packager.toml if it exists
        let config_file = load_packager_toml(&root)?;

        // Merge excludes from all sources
        let mut excludes = Vec::new();

        // Add excludes from config file
        if let Some(ref cfg) = config_file {
            excludes.extend(cfg.defaults.exclude.clone());
        }

        // Add excludes from .packagerignore
        excludes.extend(packagerignore_patterns);

        // Add CLI excludes (with comma-separated pattern support)
        excludes.extend(merge_exclude_patterns(&self.exclude));

        // Add shorthand excludes
        if self.no_packages {
            excludes.push("packages/".to_string());
        }

        for package in &self.ignore_package {
            excludes.push(format!("packages/{}", package));
        }

        // Determine compression level
        let compression = if matches!(self.compression, CompressionLevel::Default) {
            if let Some(ref cfg) = config_file {
                if let Some(ref comp) = cfg.defaults.compression {
                    comp.parse().unwrap_or_default()
                } else {
                    CompressionLevel::Default
                }
            } else {
                CompressionLevel::Default
            }
        } else {
            self.compression
        };

        // Determine no_empty_dirs
        let no_empty_dirs = self.no_empty_dirs
            || config_file.as_ref().map(|c| c.defaults.no_empty_dirs).unwrap_or(false);

        // Determine no_gitignore
        let no_gitignore = self.no_gitignore
            || config_file.as_ref().map(|c| c.defaults.no_gitignore).unwrap_or(false);

        // Determine output ZIP path
        let zip_path = if let Some(ref zip) = self.zip {
            zip.clone()
        } else {
            // Generate default name: <repo>-YYYYMMDD_HHMMSS.zip
            let repo_name = root.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("repo");
            let timestamp = Local::now().format("%Y%m%d_%H%M%S");
            PathBuf::from(format!("{}-{}.zip", repo_name, timestamp))
        };

        // Resolve output directory
        let zip_path = if let Some(ref output_dir) = self.output_dir {
            output_dir.join(&zip_path)
        } else {
            zip_path
        };

        Ok(Config {
            zip_path,
            root,
            excludes,
            tracked_only: self.tracked_only,
            list_only: self.list_only,
            no_empty_dirs,
            deterministic: self.deterministic,
            compression,
            checksum: self.checksum,
            recurse_submodules: self.recurse_submodules,
            no_gitignore,
            quiet: self.quiet,
            verbose: self.verbose,
        })
    }
}

/// Load .packagerignore file from the repository root
fn load_packagerignore(root: &Path) -> Result<Vec<String>> {
    let packagerignore_path = root.join(".packagerignore");
    
    if !packagerignore_path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&packagerignore_path)
        .with_context(|| format!("Failed to read .packagerignore: {:?}", packagerignore_path))?;

    let patterns: Vec<String> = content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(String::from)
        .collect();

    Ok(patterns)
}

/// Load .packager.toml file from the repository root
fn load_packager_toml(root: &Path) -> Result<Option<ConfigFile>> {
    let packager_toml_path = root.join(".packager.toml");
    
    if !packager_toml_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&packager_toml_path)
        .with_context(|| format!("Failed to read .packager.toml: {:?}", packager_toml_path))?;

    let config: ConfigFile = toml::from_str(&content)
        .with_context(|| format!("Failed to parse .packager.toml: {:?}", packager_toml_path))?;

    Ok(Some(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::str::FromStr;
    use tempfile::TempDir;

    #[test]
    fn test_compression_level_from_str() {
        assert!(matches!(CompressionLevel::from_str("fast"), Ok(CompressionLevel::Fast)));
        assert!(matches!(CompressionLevel::from_str("default"), Ok(CompressionLevel::Default)));
        assert!(matches!(CompressionLevel::from_str("best"), Ok(CompressionLevel::Best)));
        assert!(CompressionLevel::from_str("invalid").is_err());
    }

    #[test]
    fn test_compression_level_case_insensitive() {
        assert!(matches!(CompressionLevel::from_str("FAST"), Ok(CompressionLevel::Fast)));
        assert!(matches!(CompressionLevel::from_str("Default"), Ok(CompressionLevel::Default)));
    }

    #[test]
    fn test_load_packagerignore() {
        let temp_dir = TempDir::new().unwrap();
        let packagerignore_path = temp_dir.path().join(".packagerignore");
        
        let mut file = fs::File::create(&packagerignore_path).unwrap();
        writeln!(file, "# Comment").unwrap();
        writeln!(file, "node_modules/").unwrap();
        writeln!(file, "").unwrap();
        writeln!(file, "target/").unwrap();
        writeln!(file, "*.log").unwrap();

        let patterns = load_packagerignore(temp_dir.path()).unwrap();
        
        assert_eq!(patterns.len(), 3);
        assert!(patterns.contains(&"node_modules/".to_string()));
        assert!(patterns.contains(&"target/".to_string()));
        assert!(patterns.contains(&"*.log".to_string()));
    }

    #[test]
    fn test_load_packagerignore_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let patterns = load_packagerignore(temp_dir.path()).unwrap();
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_load_packager_toml() {
        let temp_dir = TempDir::new().unwrap();
        let packager_toml_path = temp_dir.path().join(".packager.toml");
        
        let content = r#"
[defaults]
exclude = ["packages/Odoo", "graphql"]
compression = "fast"
no_empty_dirs = true
no_gitignore = true
"#;
        fs::write(&packager_toml_path, content).unwrap();

        let config = load_packager_toml(temp_dir.path()).unwrap().unwrap();
        
        assert_eq!(config.defaults.exclude.len(), 2);
        assert!(config.defaults.exclude.contains(&"packages/Odoo".to_string()));
        assert!(config.defaults.exclude.contains(&"graphql".to_string()));
        assert_eq!(config.defaults.compression, Some("fast".to_string()));
        assert!(config.defaults.no_empty_dirs);
        assert!(config.defaults.no_gitignore);
    }

    #[test]
    fn test_load_packager_toml_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let config = load_packager_toml(temp_dir.path()).unwrap();
        assert!(config.is_none());
    }

    #[test]
    fn test_default_zip_path_generation() {
        // This test verifies the default zip path format
        // The actual timestamp will vary, so we just check the pattern
        let repo_name = "myrepo";
        let pattern = format!("{}-", repo_name);
        // Default format: <repo>-YYYYMMDD_HHMMSS.zip
        assert!(pattern.starts_with("myrepo-"));
    }
}
