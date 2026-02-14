//! Progress reporting module for user feedback
//!
//! This module provides:
//! - Progress bar abstraction using indicatif
//! - Quiet mode support
//! - Verbose mode support

use indicatif::{ProgressBar, ProgressStyle};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Progress reporter for tracking operations
pub struct ProgressReporter {
    /// Progress bar (optional, None in quiet mode)
    progress: Option<ProgressBar>,
    /// Counter for completed items
    counter: Arc<AtomicUsize>,
    /// Total items (0 for unknown)
    total: usize,
    /// Quiet mode
    quiet: bool,
    /// Verbose mode
    verbose: bool,
}

impl ProgressReporter {
    /// Create a new progress reporter
    pub fn new(total: usize, quiet: bool, verbose: bool) -> Self {
        let progress = if quiet {
            None
        } else {
            let pb = ProgressBar::new(total as u64);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                    .unwrap()
                    .progress_chars("=>-"),
            );
            Some(pb)
        };

        Self {
            progress,
            counter: Arc::new(AtomicUsize::new(0)),
            total,
            quiet,
            verbose,
        }
    }

    /// Create a spinner for indeterminate progress
    pub fn spinner(quiet: bool, verbose: bool) -> Self {
        let progress = if quiet {
            None
        } else {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg}")
                    .unwrap(),
            );
            pb.enable_steady_tick(std::time::Duration::from_millis(100));
            Some(pb)
        };

        Self {
            progress,
            counter: Arc::new(AtomicUsize::new(0)),
            total: 0,
            quiet,
            verbose,
        }
    }

    /// Increment the progress counter
    pub fn inc(&self) {
        self.counter.fetch_add(1, Ordering::Relaxed);
        if let Some(ref pb) = self.progress {
            pb.inc(1);
        }
    }

    /// Set the current message
    pub fn set_message(&self, msg: &str) {
        if let Some(ref pb) = self.progress {
            pb.set_message(msg.to_string());
        }
    }

    /// Print a message (respects quiet/verbose settings)
    pub fn println(&self, msg: &str) {
        if self.quiet {
            return;
        }

        if let Some(ref pb) = self.progress {
            pb.println(msg);
        } else {
            println!("{}", msg);
        }
    }

    /// Print a verbose message (only in verbose mode)
    pub fn verbose(&self, msg: &str) {
        if self.verbose && !self.quiet {
            if let Some(ref pb) = self.progress {
                pb.println(format!("[verbose] {}", msg));
            } else {
                println!("[verbose] {}", msg);
            }
        }
    }

    /// Print an error message (always shown)
    pub fn error(&self, msg: &str) {
        if let Some(ref pb) = self.progress {
            pb.println(format!("\x1b[31mError: {}\x1b[0m", msg));
        } else {
            eprintln!("Error: {}", msg);
        }
    }

    /// Finish the progress reporting
    pub fn finish(&self) {
        if let Some(ref pb) = self.progress {
            pb.finish();
        }
    }

    /// Finish with a message
    pub fn finish_with_message(&self, msg: &str) {
        if let Some(ref pb) = self.progress {
            pb.finish_with_message(msg.to_string());
        } else if !self.quiet {
            println!("{}", msg);
        }
    }

    /// Get the current count
    pub fn current(&self) -> usize {
        self.counter.load(Ordering::Relaxed)
    }

    /// Get the total count
    pub fn total(&self) -> usize {
        self.total
    }

    /// Check if quiet mode is enabled
    pub fn is_quiet(&self) -> bool {
        self.quiet
    }

    /// Check if verbose mode is enabled
    pub fn is_verbose(&self) -> bool {
        self.verbose
    }
}

impl Drop for ProgressReporter {
    fn drop(&mut self) {
        if let Some(ref pb) = self.progress {
            pb.finish_and_clear();
        }
    }
}

/// Simple console output helper
pub struct Console {
    quiet: bool,
    verbose: bool,
}

impl Console {
    /// Create a new console helper
    pub fn new(quiet: bool, verbose: bool) -> Self {
        Self { quiet, verbose }
    }

    /// Print a message (respects quiet mode)
    pub fn print(&self, msg: &str) {
        if !self.quiet {
            println!("{}", msg);
        }
    }

    /// Print a verbose message (only in verbose mode)
    pub fn verbose(&self, msg: &str) {
        if self.verbose && !self.quiet {
            println!("[verbose] {}", msg);
        }
    }

    /// Print an error message (always shown)
    pub fn error(&self, msg: &str) {
        eprintln!("\x1b[31mError: {}\x1b[0m", msg);
    }

    /// Print a warning message
    pub fn warn(&self, msg: &str) {
        if !self.quiet {
            eprintln!("\x1b[33mWarning: {}\x1b[0m", msg);
        }
    }

    /// Print a success message
    pub fn success(&self, msg: &str) {
        if !self.quiet {
            println!("\x1b[32m{}\x1b[0m", msg);
        }
    }

    /// Print an info message
    pub fn info(&self, msg: &str) {
        if !self.quiet {
            println!("\x1b[34mInfo: {}\x1b[0m", msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_reporter_quiet() {
        let reporter = ProgressReporter::new(100, true, false);
        
        assert!(reporter.is_quiet());
        assert!(!reporter.is_verbose());
        
        // These should not panic in quiet mode
        reporter.inc();
        reporter.set_message("test");
        reporter.println("test");
        reporter.finish();
    }

    #[test]
    fn test_progress_reporter_verbose() {
        let reporter = ProgressReporter::new(100, false, true);
        
        assert!(!reporter.is_quiet());
        assert!(reporter.is_verbose());
        
        reporter.inc();
        assert_eq!(reporter.current(), 1);
    }

    #[test]
    fn test_progress_reporter_spinner() {
        let reporter = ProgressReporter::spinner(false, false);
        
        reporter.set_message("Processing...");
        reporter.inc();
        reporter.finish();
    }

    #[test]
    fn test_console() {
        let console = Console::new(false, true);
        
        console.print("test");
        console.verbose("verbose test");
        console.error("error test");
        console.warn("warning test");
        console.success("success test");
        console.info("info test");
    }

    #[test]
    fn test_console_quiet() {
        let console = Console::new(true, false);
        
        // These should not panic in quiet mode
        console.print("test");
        console.verbose("verbose test");
        console.warn("warning test");
        console.success("success test");
        console.info("info test");
        
        // Error should still work
        console.error("error test");
    }

    #[test]
    fn test_progress_counter() {
        let reporter = ProgressReporter::new(10, true, false);
        
        assert_eq!(reporter.current(), 0);
        
        reporter.inc();
        reporter.inc();
        reporter.inc();
        
        assert_eq!(reporter.current(), 3);
    }
}
