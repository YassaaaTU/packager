//! Checksum computation module for streaming SHA256 checksums
//!
//! This module provides:
//! - Streaming SHA256 checksum computation
//! - Wrapper writer that computes checksum while writing

use sha2::{Digest, Sha256};
use std::io::{self, Seek, SeekFrom, Write};

/// A writer wrapper that computes SHA256 checksum while writing
pub struct ChecksumWriter<W> {
    /// Inner writer
    inner: W,
    /// SHA256 hasher
    hasher: Sha256,
}

impl<W: Write> ChecksumWriter<W> {
    /// Create a new checksum writer
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
        }
    }

    /// Finalize and return the checksum as a hex string
    pub fn finalize(self) -> String {
        let result = self.hasher.finalize();
        format!("{:x}", result)
    }

    /// Get the inner writer
    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for ChecksumWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.hasher.update(&buf[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<W: Write + Seek> Seek for ChecksumWriter<W> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}

/// Compute SHA256 checksum of data
pub fn compute_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_compute_sha256() {
        // Test with known value
        let checksum = compute_sha256(b"hello world");
        assert_eq!(
            checksum,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_checksum_writer() {
        let mut output = Vec::new();
        let mut writer = ChecksumWriter::new(&mut output);

        writer.write_all(b"hello").unwrap();
        writer.write_all(b" world").unwrap();

        let checksum = writer.finalize();

        assert_eq!(
            checksum,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
        assert_eq!(&output, b"hello world");
    }

    #[test]
    fn test_checksum_writer_empty() {
        let mut output = Vec::new();
        let writer = ChecksumWriter::new(&mut output);

        let checksum = writer.finalize();

        // SHA256 of empty string
        assert_eq!(
            checksum,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_checksum_writer_large_data() {
        let mut output = Vec::new();
        let mut writer = ChecksumWriter::new(&mut output);

        // Write 1MB of data
        let data = vec![0xAB; 1024 * 1024];
        writer.write_all(&data).unwrap();

        let checksum = writer.finalize();

        // Verify checksum is 64 characters (SHA256 hex)
        assert_eq!(checksum.len(), 64);
        assert!(checksum.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_checksum_writer_with_cursor() {
        let buffer = Vec::new();
        let cursor = Cursor::new(buffer);
        let mut writer = ChecksumWriter::new(cursor);

        writer.write_all(b"test data").unwrap();

        // finalize consumes the writer and returns the checksum
        // We need to verify the data was written before finalizing
        // Since finalize consumes self, we can't access inner after
        // Let's test with a different approach
        let mut output = Vec::new();
        {
            let mut writer = ChecksumWriter::new(&mut output);
            writer.write_all(b"test data").unwrap();
            let checksum = writer.finalize();
            assert_eq!(checksum.len(), 64);
        }
        assert_eq!(&output, b"test data");
    }
}
