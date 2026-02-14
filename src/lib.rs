//! Packager - A cross-platform CLI tool to package repositories into ZIP archives
//!
//! This library provides functionality to:
//! - Parse CLI arguments and configuration files
//! - Honor `.gitignore` semantics and user-specified excludes
//! - Walk directory trees in parallel for file collection
//! - Create streaming ZIP archives with deterministic output

pub mod config;
pub mod excludes;
pub mod collector;
pub mod zipper;
pub mod checksum;
pub mod progress;

pub use config::Config;
pub use excludes::ExcludeMatcher;
pub use collector::FileCollector;
pub use zipper::Zipper;
pub use checksum::ChecksumWriter;
pub use progress::ProgressReporter;

/// Exit codes for the CLI
pub mod exit_codes {
    /// Success
    pub const SUCCESS: i32 = 0;
    /// No files to package
    pub const NO_FILES: i32 = 1;
    /// I/O error
    pub const IO_ERROR: i32 = 2;
    /// Invalid arguments
    pub const INVALID_ARGS: i32 = 3;
    /// Git not found
    pub const GIT_NOT_FOUND: i32 = 4;
}
