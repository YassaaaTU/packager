//! ZIP archive creation module with streaming support
//!
//! This module provides:
//! - Streaming ZIP creation (low memory usage)
//! - Deterministic output (sorted entries, stable timestamps)
//! - Empty directory support
//! - Compression level configuration

use anyhow::{Context, Result};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::checksum::ChecksumWriter;
use crate::collector::FileCollection;
use crate::config::CompressionLevel;

/// ZIP archive writer with streaming support
pub struct Zipper {
    /// Output file path
    output_path: std::path::PathBuf,
    /// Compression method
    compression: CompressionMethod,
    /// Use deterministic timestamps
    deterministic: bool,
    /// Deterministic timestamp (fixed)
    deterministic_time: zip::DateTime,
}

impl Zipper {
    /// Create a new zipper
    pub fn new<P: AsRef<Path>>(
        output_path: P,
        compression: CompressionLevel,
        deterministic: bool,
    ) -> Self {
        let compression_method = match compression {
            CompressionLevel::Fast => CompressionMethod::Deflated,
            CompressionLevel::Default => CompressionMethod::Deflated,
            CompressionLevel::Best => CompressionMethod::Deflated,
        };

        // Use a fixed timestamp for deterministic builds
        // Unix epoch: 1980-01-01 (minimum supported by ZIP format)
        let deterministic_time = zip::DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0)
            .unwrap_or_else(|_| zip::DateTime::default());

        Self {
            output_path: output_path.as_ref().to_path_buf(),
            compression: compression_method,
            deterministic,
            deterministic_time,
        }
    }

    /// Create a ZIP archive from a file collection
    pub fn zip(&self, collection: &FileCollection) -> Result<()> {
        // Create parent directories if needed
        if let Some(parent) = self.output_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create output directory: {:?}", parent))?;
        }

        let file = File::create(&self.output_path)
            .with_context(|| format!("Failed to create ZIP file: {:?}", self.output_path))?;

        let mut writer = ZipWriter::new(file);
        let options = self.build_file_options();

        // Add files in sorted order (BTreeSet guarantees this)
        for file_path in &collection.files {
            let full_path = collection.root.join(file_path);
            self.add_file(&mut writer, &full_path, file_path, options)?;
        }

        // Add empty directories
        for dir_path in &collection.empty_dirs {
            self.add_directory(&mut writer, dir_path, options)?;
        }

        writer.finish()
            .context("Failed to finalize ZIP archive")?;

        Ok(())
    }

    /// Create a ZIP archive with checksum computation
    pub fn zip_with_checksum(&self, collection: &FileCollection) -> Result<String> {
        // Create parent directories if needed
        if let Some(parent) = self.output_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create output directory: {:?}", parent))?;
        }

        let file = File::create(&self.output_path)
            .with_context(|| format!("Failed to create ZIP file: {:?}", self.output_path))?;

        // Wrap with checksum writer
        let mut checksum_writer = ChecksumWriter::new(file);
        let mut writer = ZipWriter::new(&mut checksum_writer);
        let options = self.build_file_options();

        // Add files in sorted order
        for file_path in &collection.files {
            let full_path = collection.root.join(file_path);
            self.add_file(&mut writer, &full_path, file_path, options)?;
        }

        // Add empty directories
        for dir_path in &collection.empty_dirs {
            self.add_directory(&mut writer, dir_path, options)?;
        }

        writer.finish()
            .context("Failed to finalize ZIP archive")?;

        let checksum = checksum_writer.finalize();

        Ok(checksum)
    }

    /// Build file options for ZIP entries
    fn build_file_options(&self) -> SimpleFileOptions {
        let mut options = SimpleFileOptions::default()
            .compression_method(self.compression);

        if self.deterministic {
            options = options
                .last_modified_time(self.deterministic_time);
        } else {
            // Use current time
            if let Some(datetime) = current_datetime() {
                options = options.last_modified_time(datetime);
            }
        }

        options
    }

    /// Add a file to the ZIP archive
    fn add_file<W: Write + io::Seek>(
        &self,
        writer: &mut ZipWriter<W>,
        full_path: &Path,
        archive_path: &Path,
        options: SimpleFileOptions,
    ) -> Result<()> {
        // Normalize the archive path to use forward slashes
        let archive_name = path_to_zip_string(archive_path);

        let mut file = File::open(full_path)
            .with_context(|| format!("Failed to open file: {:?}", full_path))?;

        writer.start_file(&archive_name, options)
            .with_context(|| format!("Failed to start ZIP entry: {}", archive_name))?;

        // Stream the file content
        let mut buffer = vec![0u8; 8 * 1024]; // 8KB buffer
        loop {
            let bytes_read = file.read(&mut buffer)
                .with_context(|| format!("Failed to read file: {:?}", full_path))?;
            
            if bytes_read == 0 {
                break;
            }

            writer.write_all(&buffer[..bytes_read])
                .with_context(|| format!("Failed to write to ZIP: {}", archive_name))?;
        }

        Ok(())
    }

    /// Add a directory entry to the ZIP archive
    fn add_directory<W: Write + io::Seek>(
        &self,
        writer: &mut ZipWriter<W>,
        dir_path: &Path,
        options: SimpleFileOptions,
    ) -> Result<()> {
        // Normalize the directory path with trailing slash
        let mut dir_name = path_to_zip_string(dir_path);
        if !dir_name.ends_with('/') {
            dir_name.push('/');
        }

        writer.add_directory(&dir_name, options)
            .with_context(|| format!("Failed to add directory to ZIP: {}", dir_name))?;

        Ok(())
    }

    /// Get the output path
    pub fn output_path(&self) -> &Path {
        &self.output_path
    }
}

/// Convert a path to a ZIP-compatible string (forward slashes)
fn path_to_zip_string(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Get the current datetime in ZIP format
fn current_datetime() -> Option<zip::DateTime> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?;
    
    let secs = now.as_secs();
    
    // Convert Unix timestamp to date/time components
    // This is a simplified conversion
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = (remaining / 3600) as u8;
    let minutes = ((remaining % 3600) / 60) as u8;
    let seconds = (remaining % 60) as u8;

    // Calculate year/month/day from days since Unix epoch
    // Unix epoch is 1970-01-01
    let (year, month, day) = days_to_ymd(days as i64);

    zip::DateTime::from_date_and_time(year, month, day, hours, minutes, seconds).ok()
}

/// Convert days since Unix epoch to year/month/day
fn days_to_ymd(days: i64) -> (u16, u8, u8) {
    // Days since Unix epoch (1970-01-01)
    let mut year = 1970i64;
    let mut remaining_days = days;

    // Days in each month (non-leap year)
    let days_in_month = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    // Find the year
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    // Find the month
    let mut month = 1u8;
    let mut days_this_year = remaining_days;
    
    for (m, &days) in days_in_month.iter().enumerate() {
        let days_in_this_month = if m == 1 && is_leap_year(year) {
            days + 1
        } else {
            days
        };
        
        if days_this_year < days_in_this_month as i64 {
            month = (m + 1) as u8;
            break;
        }
        days_this_year -= days_in_this_month as i64;
    }

    let day = (days_this_year + 1) as u8;

    (year as u16, month, day)
}

/// Check if a year is a leap year
fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use std::collections::BTreeSet;

    #[test]
    fn test_path_to_zip_string() {
        assert_eq!(path_to_zip_string(Path::new("src/main.rs")), "src/main.rs");
        assert_eq!(path_to_zip_string(Path::new("Cargo.toml")), "Cargo.toml");
    }

    #[test]
    fn test_is_leap_year() {
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2023));
    }

    #[test]
    fn test_days_to_ymd() {
        // Unix epoch
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
        
        // One day later
        assert_eq!(days_to_ymd(1), (1970, 1, 2));
        
        // Some known dates
        let (y, m, d) = days_to_ymd(365); // One year later (1971-01-01)
        assert_eq!(y, 1971);
        assert_eq!(m, 1);
        assert_eq!(d, 1);
    }

    #[test]
    fn test_zipper_creates_archive() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("test.zip");
        
        // Create test files
        fs::create_dir_all(temp_dir.path().join("src")).unwrap();
        fs::write(temp_dir.path().join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(temp_dir.path().join("Cargo.toml"), "[package]").unwrap();

        let mut files = BTreeSet::new();
        files.insert(PathBuf::from("src/main.rs"));
        files.insert(PathBuf::from("Cargo.toml"));

        let collection = FileCollection {
            files,
            empty_dirs: BTreeSet::new(),
            root: temp_dir.path().to_path_buf(),
        };

        let zipper = Zipper::new(
            &output_path,
            CompressionLevel::Default,
            false,
        );

        zipper.zip(&collection).unwrap();
        
        assert!(output_path.exists());
        
        // Verify the ZIP contents
        let file = File::open(&output_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        
        assert_eq!(archive.len(), 2);
        assert!(archive.by_name("src/main.rs").is_ok());
        assert!(archive.by_name("Cargo.toml").is_ok());
    }

    #[test]
    fn test_zipper_with_empty_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("test.zip");
        
        // Create test files and empty directory
        fs::create_dir_all(temp_dir.path().join("src")).unwrap();
        fs::create_dir_all(temp_dir.path().join("empty")).unwrap();
        fs::write(temp_dir.path().join("src/main.rs"), "fn main() {}").unwrap();

        let mut files = BTreeSet::new();
        files.insert(PathBuf::from("src/main.rs"));

        let mut empty_dirs = BTreeSet::new();
        empty_dirs.insert(PathBuf::from("empty"));

        let collection = FileCollection {
            files,
            empty_dirs,
            root: temp_dir.path().to_path_buf(),
        };

        let zipper = Zipper::new(
            &output_path,
            CompressionLevel::Default,
            false,
        );

        zipper.zip(&collection).unwrap();
        
        assert!(output_path.exists());
        
        // Verify the ZIP contents
        let file = File::open(&output_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        
        assert_eq!(archive.len(), 2);
        assert!(archive.by_name("src/main.rs").is_ok());
        assert!(archive.by_name("empty/").is_ok());
    }

    #[test]
    fn test_zipper_deterministic() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("test.zip");
        
        // Create test file
        fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        let mut files = BTreeSet::new();
        files.insert(PathBuf::from("test.txt"));

        let collection = FileCollection {
            files,
            empty_dirs: BTreeSet::new(),
            root: temp_dir.path().to_path_buf(),
        };

        let zipper = Zipper::new(
            &output_path,
            CompressionLevel::Default,
            true, // deterministic
        );

        zipper.zip(&collection).unwrap();
        
        // Verify the ZIP has deterministic timestamp
        let file = File::open(&output_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        
        let entry = archive.by_name("test.txt").unwrap();
        let modified = entry.last_modified();
        
        // Should be 1980-01-01 00:00:00
        assert_eq!(modified.unwrap().year(), 1980);
        assert_eq!(modified.unwrap().month(), 1);
        assert_eq!(modified.unwrap().day(), 1);
    }

    #[test]
    fn test_zipper_with_checksum() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("test.zip");
        
        // Create test file
        fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        let mut files = BTreeSet::new();
        files.insert(PathBuf::from("test.txt"));

        let collection = FileCollection {
            files,
            empty_dirs: BTreeSet::new(),
            root: temp_dir.path().to_path_buf(),
        };

        let zipper = Zipper::new(
            &output_path,
            CompressionLevel::Default,
            false,
        );

        let checksum = zipper.zip_with_checksum(&collection).unwrap();
        
        // SHA256 checksum should be a 64-character hex string
        assert_eq!(checksum.len(), 64);
        assert!(checksum.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
