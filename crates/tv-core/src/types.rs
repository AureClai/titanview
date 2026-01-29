/// A byte range within a file, defined by offset and length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileRegion {
    pub offset: u64,
    pub length: u64,
}

impl FileRegion {
    pub fn new(offset: u64, length: u64) -> Self {
        Self { offset, length }
    }

    /// Returns the exclusive end offset of this region.
    pub fn end(&self) -> u64 {
        self.offset.saturating_add(self.length)
    }

    /// Returns true if this region overlaps with `other`.
    pub fn overlaps(&self, other: &FileRegion) -> bool {
        self.offset < other.end() && other.offset < self.end()
    }

    /// Returns true if this region fully contains `other`.
    pub fn contains(&self, other: &FileRegion) -> bool {
        self.offset <= other.offset && other.end() <= self.end()
    }
}

/// Describes what the UI is currently displaying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewPort {
    pub start: u64,
    pub visible_bytes: u64,
}

impl ViewPort {
    pub fn new(start: u64, visible_bytes: u64) -> Self {
        Self { start, visible_bytes }
    }

    /// Clamp this viewport so it stays within `[0, file_len)`.
    /// If the file is smaller than `visible_bytes`, start is set to 0
    /// and visible_bytes is capped to file_len.
    pub fn clamp(&self, file_len: u64) -> ViewPort {
        if file_len == 0 {
            return ViewPort { start: 0, visible_bytes: 0 };
        }

        let visible = self.visible_bytes.min(file_len);
        let max_start = file_len.saturating_sub(visible);
        let start = self.start.min(max_start);

        ViewPort { start, visible_bytes: visible }
    }

    /// Convert this viewport to a `FileRegion`.
    pub fn as_region(&self) -> FileRegion {
        FileRegion::new(self.start, self.visible_bytes)
    }
}

/// Classification of a 256-byte block by content type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BlockClass {
    /// Nearly all zero bytes (>95%).
    Zeros = 0,
    /// Printable ASCII text (>90% in 0x20-0x7E + whitespace).
    Ascii = 1,
    /// UTF-8 multi-byte text (lead bytes present, moderate entropy).
    Utf8 = 2,
    /// Binary structured data (low-medium entropy, non-text).
    Binary = 3,
    /// Compressed or encrypted data (Shannon entropy > 7.0).
    HighEntropy = 4,
}

impl BlockClass {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Zeros,
            1 => Self::Ascii,
            2 => Self::Utf8,
            3 => Self::Binary,
            4 => Self::HighEntropy,
            _ => Self::Binary,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Zeros => "Zeros",
            Self::Ascii => "ASCII Text",
            Self::Utf8 => "UTF-8 Text",
            Self::Binary => "Binary/Structured",
            Self::HighEntropy => "Compressed/Encrypted",
        }
    }
}

/// Results of a GPU or CPU analysis pass.
#[derive(Debug, Clone)]
pub enum AnalysisResult {
    /// Per-block Shannon entropy values (0.0 = uniform, 8.0 = max entropy).
    Entropy(Vec<f32>),
    /// File offsets where a pattern was found.
    PatternHits(Vec<u64>),
    /// Per-block content classification.
    BlockClassification(Vec<BlockClass>),
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- FileRegion tests ---

    #[test]
    fn file_region_end() {
        let r = FileRegion::new(10, 20);
        assert_eq!(r.end(), 30);
    }

    #[test]
    fn file_region_end_saturates() {
        let r = FileRegion::new(u64::MAX - 5, 10);
        assert_eq!(r.end(), u64::MAX);
    }

    #[test]
    fn file_region_overlaps_partial() {
        let a = FileRegion::new(0, 10);
        let b = FileRegion::new(5, 10);
        assert!(a.overlaps(&b));
        assert!(b.overlaps(&a));
    }

    #[test]
    fn file_region_overlaps_contained() {
        let outer = FileRegion::new(0, 100);
        let inner = FileRegion::new(10, 20);
        assert!(outer.overlaps(&inner));
        assert!(inner.overlaps(&outer));
    }

    #[test]
    fn file_region_no_overlap_adjacent() {
        let a = FileRegion::new(0, 10);
        let b = FileRegion::new(10, 10);
        assert!(!a.overlaps(&b));
        assert!(!b.overlaps(&a));
    }

    #[test]
    fn file_region_no_overlap_disjoint() {
        let a = FileRegion::new(0, 5);
        let b = FileRegion::new(100, 5);
        assert!(!a.overlaps(&b));
    }

    #[test]
    fn file_region_contains() {
        let outer = FileRegion::new(0, 100);
        let inner = FileRegion::new(10, 20);
        assert!(outer.contains(&inner));
        assert!(!inner.contains(&outer));
    }

    #[test]
    fn file_region_contains_self() {
        let r = FileRegion::new(5, 10);
        assert!(r.contains(&r));
    }

    #[test]
    fn file_region_zero_length() {
        let r = FileRegion::new(10, 0);
        assert_eq!(r.end(), 10);
        assert!(!r.overlaps(&FileRegion::new(10, 10)));
    }

    // --- ViewPort tests ---

    #[test]
    fn viewport_clamp_within_bounds() {
        let vp = ViewPort::new(100, 50);
        let clamped = vp.clamp(1000);
        assert_eq!(clamped.start, 100);
        assert_eq!(clamped.visible_bytes, 50);
    }

    #[test]
    fn viewport_clamp_start_past_end() {
        let vp = ViewPort::new(900, 200);
        let clamped = vp.clamp(1000);
        assert_eq!(clamped.start, 800);
        assert_eq!(clamped.visible_bytes, 200);
    }

    #[test]
    fn viewport_clamp_visible_exceeds_file() {
        let vp = ViewPort::new(0, 5000);
        let clamped = vp.clamp(100);
        assert_eq!(clamped.start, 0);
        assert_eq!(clamped.visible_bytes, 100);
    }

    #[test]
    fn viewport_clamp_empty_file() {
        let vp = ViewPort::new(50, 100);
        let clamped = vp.clamp(0);
        assert_eq!(clamped.start, 0);
        assert_eq!(clamped.visible_bytes, 0);
    }

    #[test]
    fn viewport_as_region() {
        let vp = ViewPort::new(10, 20);
        let r = vp.as_region();
        assert_eq!(r.offset, 10);
        assert_eq!(r.length, 20);
    }
}
