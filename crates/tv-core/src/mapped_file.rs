use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result};
use memmap2::Mmap;

use crate::types::FileRegion;

/// A memory-mapped file providing zero-copy byte slices.
pub struct MappedFile {
    mmap: Mmap,
    len: u64,
}

impl MappedFile {
    /// Open and memory-map a file.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)
            .with_context(|| format!("failed to open {}", path.display()))?;

        let metadata = file.metadata()
            .with_context(|| format!("failed to read metadata for {}", path.display()))?;

        let len = metadata.len();

        // SAFETY: We keep the file handle alive via the Mmap.
        // The file must not be modified externally while mapped.
        // SAFETY: The file must not be modified externally while mapped.
        let mmap = unsafe { Mmap::map(&file) }
            .with_context(|| format!("failed to mmap {}", path.display()))?;

        Ok(Self { mmap, len })
    }

    /// Total file size in bytes.
    pub fn len(&self) -> u64 {
        self.len
    }

    /// Returns true if the file is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get a zero-copy byte slice for the given region.
    /// Returns an empty slice if the region is out of bounds.
    pub fn slice(&self, region: FileRegion) -> &[u8] {
        let start = region.offset as usize;
        let end = region.end().min(self.len) as usize;

        if start >= self.mmap.len() || start >= end {
            return &[];
        }

        &self.mmap[start..end]
    }

    /// Get a byte slice starting at `offset` with at most `len` bytes.
    pub fn slice_at(&self, offset: u64, len: u64) -> &[u8] {
        self.slice(FileRegion::new(offset, len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_fixture(data: &[u8]) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("failed to create temp file");
        f.write_all(data).expect("failed to write fixture");
        f.flush().expect("failed to flush");
        f
    }

    #[test]
    fn open_and_read_small_file() {
        let data = b"Hello, TitanView!";
        let f = create_fixture(data);

        let mf = MappedFile::open(f.path()).unwrap();
        assert_eq!(mf.len(), data.len() as u64);

        let slice = mf.slice(FileRegion::new(0, data.len() as u64));
        assert_eq!(slice, data);
    }

    #[test]
    fn slice_at_offset() {
        let data = b"ABCDEFGHIJKLMNOP";
        let f = create_fixture(data);
        let mf = MappedFile::open(f.path()).unwrap();

        let slice = mf.slice(FileRegion::new(4, 4));
        assert_eq!(slice, b"EFGH");
    }

    #[test]
    fn slice_past_eof_is_truncated() {
        let data = b"short";
        let f = create_fixture(data);
        let mf = MappedFile::open(f.path()).unwrap();

        let slice = mf.slice(FileRegion::new(3, 100));
        assert_eq!(slice, b"rt");
    }

    #[test]
    fn slice_completely_out_of_bounds() {
        let data = b"data";
        let f = create_fixture(data);
        let mf = MappedFile::open(f.path()).unwrap();

        let slice = mf.slice(FileRegion::new(100, 10));
        assert!(slice.is_empty());
    }

    #[test]
    fn slice_at_helper() {
        let data = b"0123456789";
        let f = create_fixture(data);
        let mf = MappedFile::open(f.path()).unwrap();

        assert_eq!(mf.slice_at(2, 3), b"234");
    }

    #[test]
    fn known_4kb_fixture() {
        // Create a 4096-byte fixture with a known pattern
        let mut data = vec![0u8; 4096];
        for (i, byte) in data.iter_mut().enumerate() {
            *byte = (i % 256) as u8;
        }
        let f = create_fixture(&data);
        let mf = MappedFile::open(f.path()).unwrap();

        assert_eq!(mf.len(), 4096);

        // Check first 16 bytes
        assert_eq!(mf.slice_at(0, 16), &data[0..16]);
        // Check bytes at offset 256
        assert_eq!(mf.slice_at(256, 16), &data[256..272]);
        // Check last 16 bytes
        assert_eq!(mf.slice_at(4080, 16), &data[4080..4096]);
    }
}
