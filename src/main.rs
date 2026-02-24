//! Packager - A cross-platform CLI tool to package repositories into ZIP archives
//!
//! Usage:
//!   packager [OPTIONS] [PATH]
//!
//! Examples:
//!   packager                          # Package current directory
//!   packager -z release.zip           # Output to specific file
//!   packager -e node_modules -e dist  # Exclude patterns
//!   packager --tracked-only           # Only git-tracked files
//!   packager --list-only              # List files without creating ZIP

use anyhow::{Context, Result};
use tracing_subscriber::EnvFilter;

use packager::{
    config::Args,
    collector::{FileCollector, is_git_available},
    excludes::ExcludeMatcher,
    progress::Console,
    zipper::Zipper,
    exit_codes,
};

fn main() {
    // Initialize logging
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn"));
    
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .init();

    // Parse arguments
    let args = Args::parse_args();

    // Run the application
    let exit_code = match run(args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("Error: {}", e);
            if let Some(cause) = e.chain().nth(1) {
                eprintln!("Caused by: {}", cause);
            }
            exit_codes::IO_ERROR
        }
    };

    std::process::exit(exit_code);
}

fn run(args: Args) -> Result<i32> {
    // Load configuration
    let config = args.load_config()
        .context("Failed to load configuration")?;

    // Initialize console output
    let console = Console::new(config.quiet, config.verbose);

    // Validate root directory
    if !config.root.exists() {
        console.error(&format!("Directory does not exist: {:?}", config.root));
        return Ok(exit_codes::INVALID_ARGS);
    }

    if !config.root.is_dir() {
        console.error(&format!("Not a directory: {:?}", config.root));
        return Ok(exit_codes::INVALID_ARGS);
    }

    // Log configuration
    console.verbose(&format!("Root directory: {:?}", config.root));
    console.verbose(&format!("Output file: {:?}", config.zip_path));
    console.verbose(&format!("Excludes: {:?}", config.excludes));
    console.verbose(&format!("Tracked only: {}", config.tracked_only));
    console.verbose(&format!("Deterministic: {}", config.deterministic));
    console.verbose(&format!("No gitignore: {}", config.no_gitignore));

    // Warn about conflicting flags
    if config.tracked_only && config.no_gitignore {
        console.warn("--no-gitignore has no effect when --tracked-only is set");
    }

    // Check git availability
    if config.tracked_only && !is_git_available() {
        console.error("Git is required for --tracked-only but is not available");
        return Ok(exit_codes::GIT_NOT_FOUND);
    }

    // Create exclude matcher
    let exclude_matcher = ExcludeMatcher::new(config.excludes.clone())
        .context("Failed to create exclude matcher")?;

    console.verbose(&format!("Exclude patterns: {:?}", exclude_matcher.patterns()));

    // Collect files
    console.info("Collecting files...");
    
    let collector = FileCollector::new(
        config.root.clone(),
        config.tracked_only,
        config.recurse_submodules,
        exclude_matcher,
        !config.no_empty_dirs,
        config.no_gitignore,
    );

    let collection = collector.collect()
        .context("Failed to collect files")?;

    // Check if we have any files
    if collection.is_empty() {
        console.error("No files to package");
        return Ok(exit_codes::NO_FILES);
    }

    console.verbose(&format!("Found {} files", collection.file_count()));
    if !config.no_empty_dirs {
        console.verbose(&format!("Found {} empty directories", collection.empty_dir_count()));
    }

    // List-only mode
    if config.list_only {
        console.print("\nFiles to be packaged:");
        for file in &collection.files {
            println!("  {}", file.display());
        }
        if !config.no_empty_dirs && !collection.empty_dirs.is_empty() {
            console.print("\nEmpty directories:");
            for dir in &collection.empty_dirs {
                println!("  {}/", dir.display());
            }
        }
        return Ok(exit_codes::SUCCESS);
    }

    // Create ZIP archive
    console.info(&format!("Creating ZIP archive: {:?}", config.zip_path));

    let zipper = Zipper::new(
        &config.zip_path,
        config.compression,
        config.deterministic,
    );

    if config.checksum {
        let checksum = zipper.zip_with_checksum(&collection)
            .context("Failed to create ZIP archive")?;
        console.success(&format!("Created: {:?}", config.zip_path));
        console.print(&format!("SHA256: {}", checksum));
    } else {
        zipper.zip(&collection)
            .context("Failed to create ZIP archive")?;
        console.success(&format!("Created: {:?}", config.zip_path));
    }

    // Print summary
    if !config.quiet {
        let output_path = config.zip_path.canonicalize()
            .unwrap_or_else(|_| config.zip_path.clone());
        
        let metadata = std::fs::metadata(&output_path)
            .context("Failed to get output file metadata")?;
        
        let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
        
        console.print(&format!(
            "Packaged {} files ({:.2} MB)",
            collection.file_count(),
            size_mb
        ));
    }

    Ok(exit_codes::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::path::PathBuf;

    #[test]
    fn test_args_parsing() {
        let args = Args::try_parse_from(["packager", "--list-only"]);
        assert!(args.is_ok());
        assert!(args.unwrap().list_only);
    }

    #[test]
    fn test_args_with_excludes() {
        let args = Args::try_parse_from(["packager", "-e", "node_modules", "-e", "target"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        assert_eq!(args.exclude.len(), 2);
    }

    #[test]
    fn test_args_with_zip() {
        let args = Args::try_parse_from(["packager", "-z", "output.zip"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        assert_eq!(args.zip.unwrap(), PathBuf::from("output.zip"));
    }

    #[test]
    fn test_args_shorthand_no_packages() {
        let args = Args::try_parse_from(["packager", "--no-packages"]);
        assert!(args.is_ok());
        assert!(args.unwrap().no_packages);
    }

    #[test]
    fn test_args_ignore_package() {
        let args = Args::try_parse_from(["packager", "--ignore-package", "Odoo", "--ignore-package", "GraphQL"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        assert_eq!(args.ignore_package.len(), 2);
    }

    #[test]
    fn test_args_compression() {
        let args = Args::try_parse_from(["packager", "--compression", "fast"]);
        assert!(args.is_ok());
    }
}
