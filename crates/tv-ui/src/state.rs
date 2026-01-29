use std::collections::{HashSet, HashMap};
use std::path::PathBuf;
use tv_core::{MappedFile, ViewPort};
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use egui::Color32;

/// Tab selection for signatures window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SignaturesTab {
    #[default]
    QuickScan,
    DeepScan,
}

/// State for binary diff comparison.
pub struct DiffState {
    /// Second file for comparison.
    pub file_b: Option<LoadedFile>,
    /// Viewport for the second file (synced or independent).
    pub viewport_b: ViewPort,
    /// Whether viewports are synchronized.
    pub sync_scroll: bool,
    /// Diff results: offsets where bytes differ (up to first N differences for performance).
    pub diff_offsets: Option<Vec<u64>>,
    /// Total number of differences found.
    pub diff_count: u64,
    /// Whether diff computation is in progress.
    pub computing: bool,
    /// Diff computation time in ms.
    pub compute_time_ms: Option<f64>,
    /// Highlighted diff offset (for navigation).
    pub selected_diff: Option<usize>,
    /// Set of diff offsets for fast lookup (viewport-scoped).
    pub highlight_set: HashSet<u64>,
    /// Viewport range the highlight_set was built for (to avoid rebuilding every frame).
    pub highlight_viewport: (u64, u64),
    /// Current scroll offset for synchronized scrolling.
    pub scroll_offset: f32,
    /// Whether diff mode is active (split view in main window).
    pub active: bool,
}

impl Default for DiffState {
    fn default() -> Self {
        Self {
            file_b: None,
            viewport_b: ViewPort::new(0, 0),
            sync_scroll: true,
            diff_offsets: None,
            diff_count: 0,
            computing: false,
            compute_time_ms: None,
            selected_diff: None,
            highlight_set: HashSet::new(),
            highlight_viewport: (0, 0),
            scroll_offset: 0.0,
            active: false,
        }
    }
}

impl DiffState {
    /// Clear diff results.
    pub fn clear(&mut self) {
        self.diff_offsets = None;
        self.diff_count = 0;
        self.computing = false;
        self.compute_time_ms = None;
        self.selected_diff = None;
        self.highlight_set.clear();
        self.highlight_viewport = (0, 0);
        self.scroll_offset = 0.0;
    }

    /// Close the comparison file.
    pub fn close_file_b(&mut self) {
        self.file_b = None;
        self.clear();
    }

    /// Get file B length.
    pub fn file_b_len(&self) -> u64 {
        self.file_b.as_ref().map_or(0, |f| f.mapped.len())
    }

    /// Get file B name.
    pub fn file_b_name(&self) -> &str {
        self.file_b
            .as_ref()
            .and_then(|f| f.path.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("No file")
    }

    /// Rebuild highlight set for viewport range.
    /// Uses caching to avoid rebuilding every frame when viewport hasn't changed.
    pub fn rebuild_highlights_for_viewport(&mut self, vp_start: u64, vp_end: u64) {
        // Skip if already built for this viewport
        if self.highlight_viewport == (vp_start, vp_end) && !self.highlight_set.is_empty() {
            return;
        }

        self.highlight_set.clear();
        self.highlight_viewport = (vp_start, vp_end);

        let offsets = match &self.diff_offsets {
            Some(o) => o,
            None => return,
        };

        // Binary search for first offset in range
        let start_idx = offsets.partition_point(|&o| o < vp_start);

        for &offset in &offsets[start_idx..] {
            if offset >= vp_end {
                break;
            }
            self.highlight_set.insert(offset);
        }
    }

    /// Force rebuild highlights (e.g., when diff results change).
    pub fn invalidate_highlights(&mut self) {
        self.highlight_viewport = (u64::MAX, 0);
        self.highlight_set.clear();
    }
}

/// State for hex editing feature.
/// DANGEROUS: Modifying binary files can corrupt them permanently.
pub struct EditState {
    /// Whether edit mode is enabled.
    pub enabled: bool,
    /// Whether the enable confirmation dialog is open.
    pub confirm_dialog_open: bool,
    /// Pending edits: offset -> new byte value.
    pub pending_edits: HashMap<u64, u8>,
    /// Currently selected byte offset for editing.
    pub selected_offset: Option<u64>,
    /// Input buffer for hex byte entry.
    pub input_buffer: String,
    /// Whether the save confirmation dialog is open.
    pub save_dialog_open: bool,
    /// Status message (message, is_error, timestamp).
    pub status_message: Option<(String, bool)>,
    /// Original bytes before editing (for undo).
    pub original_bytes: HashMap<u64, u8>,
}

impl Default for EditState {
    fn default() -> Self {
        Self {
            enabled: false,
            confirm_dialog_open: false,
            pending_edits: HashMap::new(),
            selected_offset: None,
            input_buffer: String::new(),
            save_dialog_open: false,
            status_message: None,
            original_bytes: HashMap::new(),
        }
    }
}

impl EditState {
    /// Check if there are unsaved changes.
    pub fn has_changes(&self) -> bool {
        !self.pending_edits.is_empty()
    }

    /// Get the number of pending edits.
    pub fn edit_count(&self) -> usize {
        self.pending_edits.len()
    }

    /// Add or update an edit at the given offset.
    pub fn set_byte(&mut self, offset: u64, original: u8, new_value: u8) {
        // Store original value if not already stored
        self.original_bytes.entry(offset).or_insert(original);

        // If new value equals original, remove the edit
        if let Some(&orig) = self.original_bytes.get(&offset) {
            if new_value == orig {
                self.pending_edits.remove(&offset);
                return;
            }
        }

        self.pending_edits.insert(offset, new_value);
    }

    /// Get the edited byte value at offset, or None if not edited.
    pub fn get_edited_byte(&self, offset: u64) -> Option<u8> {
        self.pending_edits.get(&offset).copied()
    }

    /// Undo a single edit.
    pub fn undo_edit(&mut self, offset: u64) {
        self.pending_edits.remove(&offset);
    }

    /// Undo all edits.
    pub fn undo_all(&mut self) {
        self.pending_edits.clear();
    }

    /// Clear all state (when closing file or disabling edit mode).
    pub fn clear(&mut self) {
        self.enabled = false;
        self.confirm_dialog_open = false;
        self.pending_edits.clear();
        self.selected_offset = None;
        self.input_buffer.clear();
        self.save_dialog_open = false;
        self.status_message = None;
        self.original_bytes.clear();
    }

    /// Save pending edits to file.
    /// Returns (success_count, errors).
    pub fn save_to_file(&mut self, path: &std::path::Path) -> Result<usize, String> {
        if self.pending_edits.is_empty() {
            return Ok(0);
        }

        let mut file = OpenOptions::new()
            .write(true)
            .open(path)
            .map_err(|e| format!("Failed to open file for writing: {}", e))?;

        let mut saved_count = 0;
        let mut errors = Vec::new();

        // Sort edits by offset for sequential writes (better performance)
        let mut edits: Vec<_> = self.pending_edits.iter().collect();
        edits.sort_by_key(|(offset, _)| *offset);

        for (&offset, &byte) in edits {
            if let Err(e) = file.seek(SeekFrom::Start(offset)) {
                errors.push(format!("Seek to 0x{:X} failed: {}", offset, e));
                continue;
            }
            if let Err(e) = file.write_all(&[byte]) {
                errors.push(format!("Write at 0x{:X} failed: {}", offset, e));
                continue;
            }
            saved_count += 1;
        }

        if let Err(e) = file.flush() {
            return Err(format!("Failed to flush file: {}", e));
        }

        if !errors.is_empty() {
            return Err(format!("Saved {} bytes, but {} errors: {}",
                saved_count, errors.len(), errors.join("; ")));
        }

        // Clear pending edits after successful save
        self.pending_edits.clear();
        self.original_bytes.clear();

        Ok(saved_count)
    }
}

/// Central application state shared across all panels.
pub struct AppState {
    /// Currently opened file (if any).
    pub file: Option<LoadedFile>,
    /// Current viewport into the file.
    pub viewport: ViewPort,
    /// Per-block entropy results from GPU (if computed).
    pub entropy: Option<Vec<f32>>,
    /// Per-block classification results from GPU (if computed).
    /// Each u8 maps to `BlockClass::from_u8()`.
    pub classification: Option<Vec<u8>>,
    /// Search state.
    pub search: SearchState,
    /// "Go to offset" dialog state.
    pub goto_open: bool,
    pub goto_text: String,
    /// Detected file signatures (quick scan at startup, first 1 MB).
    pub signatures: Option<Vec<SignatureHit>>,
    /// Deep scan state (GPU multi-pattern, full file).
    pub deep_scan: DeepScanState,
    /// Cached entropy stats (avg, computed once when data arrives).
    pub cached_entropy_stats: Option<EntropyStats>,
    /// Cached classification counts (computed once when data arrives).
    pub cached_class_counts: Option<[u32; 5]>,
    /// Current tab in signatures window.
    pub signatures_tab: SignaturesTab,
    /// Binary diff state.
    pub diff: DiffState,
    /// Structure inspector highlights (offsets to highlight).
    pub inspector_highlights: HashSet<u64>,
    /// Hex editing state (DANGEROUS operation).
    pub edit: EditState,
    /// Cached minimap pixels (avoid recomputing 16M+ block iterations every frame).
    pub minimap_cache: MinimapCache,
}

/// Cached entropy statistics to avoid recomputing every frame.
#[derive(Clone, Copy)]
pub struct EntropyStats {
    pub avg: f32,
    pub block_count: usize,
}

/// Cached minimap pixels to avoid recomputing downsampling every frame.
/// For a 4GB file with 256-byte blocks, there are 16M blocks.
/// Without caching, we iterate through all blocks every frame (34M+ iterations).
/// With caching, we only recompute when data or height changes.
#[derive(Default)]
pub struct MinimapCache {
    /// Pre-computed pixel colors for the minimap.
    pub pixels: Vec<Color32>,
    /// Height (in pixels) this cache was computed for.
    pub cached_height: usize,
    /// Number of entropy blocks when cache was computed.
    pub cached_block_count: usize,
    /// Whether classification was available when cache was computed.
    pub cached_has_classification: bool,
}

impl MinimapCache {
    /// Check if the cache is valid for the current state.
    pub fn is_valid(&self, height: usize, block_count: usize, has_classification: bool) -> bool {
        !self.pixels.is_empty()
            && self.cached_height == height
            && self.cached_block_count == block_count
            && self.cached_has_classification == has_classification
    }

    /// Invalidate the cache (call when entropy/classification data changes).
    pub fn invalidate(&mut self) {
        self.pixels.clear();
        self.cached_height = 0;
        self.cached_block_count = 0;
    }
}

/// State for the pattern search feature.
pub struct SearchState {
    /// Hex input string (e.g. "FF D8 FF").
    pub query_text: String,
    /// Parsed pattern bytes (set after successful parse).
    pub pattern: Option<Vec<u8>>,
    /// Whether a search is currently running.
    pub searching: bool,
    /// Match offsets found by the GPU scan.
    pub results: Option<Vec<u64>>,
    /// Currently selected result index (for navigation).
    pub selected_result: Option<usize>,
    /// Pre-computed set of highlighted byte offsets for the CURRENT VIEWPORT only.
    pub highlight_set: HashSet<u64>,
    /// Viewport range the highlight_set was built for (to avoid rebuilding every frame).
    pub highlight_viewport: (u64, u64),
    /// Search duration in milliseconds.
    pub search_duration_ms: Option<f64>,
}

/// A detected file signature (magic bytes).
#[derive(Debug, Clone)]
pub struct SignatureHit {
    pub offset: u64,
    pub name: String,
    pub magic: Vec<u8>,
}

/// Sort order for deep scan results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SignatureSortOrder {
    #[default]
    OffsetAsc,
    OffsetDesc,
    NameAsc,
    NameDesc,
    TypeAsc,
}

/// Signature type category for filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureCategory {
    All,
    Executables,
    Archives,
    Images,
    Documents,
    Databases,
    Other,
}

impl Default for SignatureCategory {
    fn default() -> Self {
        Self::All
    }
}

impl SignatureCategory {
    /// Check if a signature name matches this category.
    pub fn matches(&self, name: &str) -> bool {
        let name_lower = name.to_lowercase();
        match self {
            Self::All => true,
            Self::Executables => {
                name_lower.contains("exe") || name_lower.contains("elf")
                || name_lower.contains("mach") || name_lower.contains("dll")
                || name_lower.contains("mz") || name_lower.contains("pe")
            }
            Self::Archives => {
                name_lower.contains("zip") || name_lower.contains("gz")
                || name_lower.contains("7z") || name_lower.contains("rar")
                || name_lower.contains("tar") || name_lower.contains("bz2")
                || name_lower.contains("xz") || name_lower.contains("cab")
            }
            Self::Images => {
                name_lower.contains("png") || name_lower.contains("jpg")
                || name_lower.contains("jpeg") || name_lower.contains("gif")
                || name_lower.contains("bmp") || name_lower.contains("webp")
                || name_lower.contains("ico") || name_lower.contains("tiff")
            }
            Self::Documents => {
                name_lower.contains("pdf") || name_lower.contains("doc")
                || name_lower.contains("xml") || name_lower.contains("rtf")
                || name_lower.contains("odf") || name_lower.contains("xls")
            }
            Self::Databases => {
                name_lower.contains("sqlite") || name_lower.contains("db")
                || name_lower.contains("sql")
            }
            Self::Other => {
                !Self::Executables.matches(name) && !Self::Archives.matches(name)
                && !Self::Images.matches(name) && !Self::Documents.matches(name)
                && !Self::Databases.matches(name)
            }
        }
    }

    /// Display name for UI.
    pub fn label(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Executables => "Executables",
            Self::Archives => "Archives",
            Self::Images => "Images",
            Self::Documents => "Documents",
            Self::Databases => "Databases",
            Self::Other => "Other",
        }
    }
}

/// State for the GPU deep scan feature (multi-pattern signature detection).
#[derive(Default)]
pub struct DeepScanState {
    /// Whether a deep scan is currently running.
    pub scanning: bool,
    /// Results from the deep scan: all signatures found throughout the file.
    pub results: Option<Vec<SignatureHit>>,
    /// Filtered and sorted indices into results (avoids re-sorting original).
    pub filtered_indices: Vec<usize>,
    /// Current sort order.
    pub sort_order: SignatureSortOrder,
    /// Current filter category.
    pub filter_category: SignatureCategory,
    /// Text filter (search in signature name).
    pub filter_text: String,
    /// Scan duration in milliseconds.
    pub duration_ms: Option<f64>,
    /// Currently selected result index (index into filtered_indices).
    pub selected_result: Option<usize>,
    /// Progress: bytes scanned so far.
    pub bytes_scanned: u64,
    /// Progress: total bytes to scan.
    pub total_bytes: u64,
    /// Pre-computed set of highlighted byte offsets for the selected signature.
    pub highlight_set: HashSet<u64>,
}

impl DeepScanState {
    /// Update highlight set when a signature is selected.
    /// selected_result is an index into filtered_indices.
    pub fn update_highlight(&mut self) {
        self.highlight_set.clear();

        let selected_idx = match self.selected_result {
            Some(idx) => idx,
            None => return,
        };

        // Get the actual result index from filtered_indices
        let actual_idx = match self.filtered_indices.get(selected_idx) {
            Some(&idx) => idx,
            None => return,
        };

        let results = match &self.results {
            Some(r) => r,
            None => return,
        };

        if let Some(sig) = results.get(actual_idx) {
            let sig_len = sig.magic.len() as u64;
            for i in 0..sig_len {
                self.highlight_set.insert(sig.offset + i);
            }
        }
    }

    /// Clear highlight when deselecting.
    pub fn clear_highlight(&mut self) {
        self.highlight_set.clear();
    }

    /// Get the signature at the given filtered index.
    pub fn get_filtered_signature(&self, filtered_idx: usize) -> Option<&SignatureHit> {
        let actual_idx = *self.filtered_indices.get(filtered_idx)?;
        self.results.as_ref()?.get(actual_idx)
    }

    /// Rebuild filtered_indices based on current filter and sort settings.
    /// Call this after changing filter_category, filter_text, or sort_order.
    pub fn rebuild_filtered_indices(&mut self) {
        self.filtered_indices.clear();
        self.selected_result = None;

        let results = match &self.results {
            Some(r) => r,
            None => return,
        };

        // Build list of indices matching the filter
        let filter_text_lower = self.filter_text.to_lowercase();

        for (i, sig) in results.iter().enumerate() {
            // Category filter
            if !self.filter_category.matches(&sig.name) {
                continue;
            }

            // Text filter
            if !filter_text_lower.is_empty() && !sig.name.to_lowercase().contains(&filter_text_lower) {
                continue;
            }

            self.filtered_indices.push(i);
        }

        // Sort filtered indices
        let results_ref = results;
        match self.sort_order {
            SignatureSortOrder::OffsetAsc => {
                self.filtered_indices.sort_by_key(|&i| results_ref[i].offset);
            }
            SignatureSortOrder::OffsetDesc => {
                self.filtered_indices.sort_by_key(|&i| std::cmp::Reverse(results_ref[i].offset));
            }
            SignatureSortOrder::NameAsc => {
                self.filtered_indices.sort_by(|&a, &b| results_ref[a].name.cmp(&results_ref[b].name));
            }
            SignatureSortOrder::NameDesc => {
                self.filtered_indices.sort_by(|&a, &b| results_ref[b].name.cmp(&results_ref[a].name));
            }
            SignatureSortOrder::TypeAsc => {
                self.filtered_indices.sort_by(|&a, &b| {
                    signature_type_order(&results_ref[a].name).cmp(&signature_type_order(&results_ref[b].name))
                });
            }
        }
    }

    /// Get count of filtered results.
    pub fn filtered_count(&self) -> usize {
        self.filtered_indices.len()
    }

    /// Get total count of all results.
    pub fn total_count(&self) -> usize {
        self.results.as_ref().map_or(0, |r| r.len())
    }
}

/// Helper to get a numeric order for signature types (for sorting).
fn signature_type_order(name: &str) -> u8 {
    if SignatureCategory::Executables.matches(name) { 0 }
    else if SignatureCategory::Archives.matches(name) { 1 }
    else if SignatureCategory::Images.matches(name) { 2 }
    else if SignatureCategory::Documents.matches(name) { 3 }
    else if SignatureCategory::Databases.matches(name) { 4 }
    else { 5 }
}

/// A file that has been opened and memory-mapped.
pub struct LoadedFile {
    pub path: PathBuf,
    pub mapped: MappedFile,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            query_text: String::new(),
            pattern: None,
            searching: false,
            results: None,
            selected_result: None,
            highlight_set: HashSet::new(),
            highlight_viewport: (0, 0),
            search_duration_ms: None,
        }
    }
}

impl SearchState {
    /// Rebuild the highlight set for the given viewport range only.
    /// Uses binary search on the sorted results to find relevant matches.
    /// Call this when results change OR when the viewport moves.
    pub fn rebuild_highlights_for_viewport(&mut self, vp_start: u64, vp_end: u64) {
        // Skip if already built for this viewport
        if self.highlight_viewport == (vp_start, vp_end) && !self.highlight_set.is_empty() {
            return;
        }

        self.highlight_set.clear();
        self.highlight_viewport = (vp_start, vp_end);

        let results = match &self.results {
            Some(r) => r,
            None => return,
        };
        let pat_len = self.pattern.as_ref().map_or(0, |p| p.len()) as u64;
        if pat_len == 0 || results.is_empty() {
            return;
        }

        // Binary search for the first result that could overlap the viewport
        let search_start = vp_start.saturating_sub(pat_len);
        let start_idx = results.partition_point(|&o| o < search_start);

        for &offset in &results[start_idx..] {
            if offset >= vp_end {
                break;
            }
            // This match overlaps the viewport
            for i in 0..pat_len {
                let byte = offset + i;
                if byte >= vp_start && byte < vp_end {
                    self.highlight_set.insert(byte);
                }
            }
        }
    }

    /// Force rebuild (e.g., when new results arrive).
    pub fn rebuild_highlights(&mut self) {
        // Reset viewport tracking so next frame rebuilds
        self.highlight_viewport = (u64::MAX, 0);
        self.highlight_set.clear();
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            file: None,
            viewport: ViewPort::new(0, 0),
            entropy: None,
            classification: None,
            search: SearchState::default(),
            goto_open: false,
            goto_text: String::new(),
            signatures: None,
            deep_scan: DeepScanState::default(),
            cached_entropy_stats: None,
            cached_class_counts: None,
            signatures_tab: SignaturesTab::default(),
            diff: DiffState::default(),
            inspector_highlights: HashSet::new(),
            edit: EditState::default(),
            minimap_cache: MinimapCache::default(),
        }
    }
}

/// Parse a hex string like "FF D8 FF E0" into bytes.
/// Accepts spaces, commas, or no separator. Also accepts "0x" prefix per byte.
pub fn parse_hex_pattern(input: &str) -> Result<Vec<u8>, String> {
    let cleaned = input.trim();
    if cleaned.is_empty() {
        return Err("Empty pattern".to_string());
    }

    let mut bytes = Vec::new();
    // Split on whitespace or commas
    for token in cleaned.split(|c: char| c.is_whitespace() || c == ',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let hex = token.strip_prefix("0x").or_else(|| token.strip_prefix("0X")).unwrap_or(token);
        if hex.len() != 2 {
            return Err(format!("Invalid hex byte: '{}'", token));
        }
        let b = u8::from_str_radix(hex, 16)
            .map_err(|_| format!("Invalid hex byte: '{}'", token))?;
        bytes.push(b);
    }

    if bytes.is_empty() {
        return Err("No valid bytes found".to_string());
    }
    if bytes.len() > 16 {
        return Err(format!("Pattern too long ({} bytes, max 16)", bytes.len()));
    }
    Ok(bytes)
}

impl AppState {
    /// Returns the file size in bytes, or 0 if no file is loaded.
    pub fn file_len(&self) -> u64 {
        self.file.as_ref().map_or(0, |f| f.mapped.len())
    }

    /// Returns the file path as a string, or "No file" if none is loaded.
    pub fn file_name(&self) -> &str {
        self.file
            .as_ref()
            .and_then(|f| f.path.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("No file")
    }

    /// Returns the full file path as a string, or empty.
    pub fn file_path_display(&self) -> String {
        self.file
            .as_ref()
            .map(|f| f.path.display().to_string())
            .unwrap_or_default()
    }

    /// Returns true if a file is currently loaded.
    pub fn has_file(&self) -> bool {
        self.file.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_has_no_file() {
        let state = AppState::default();
        assert!(!state.has_file());
        assert_eq!(state.file_len(), 0);
        assert_eq!(state.file_name(), "No file");
        assert!(state.file_path_display().is_empty());
        assert!(state.entropy.is_none());
        assert!(state.classification.is_none());
        assert!(state.search.results.is_none());
    }

    #[test]
    fn parse_hex_basic() {
        assert_eq!(parse_hex_pattern("FF D8 FF").unwrap(), vec![0xFF, 0xD8, 0xFF]);
    }

    #[test]
    fn parse_hex_with_0x_prefix() {
        assert_eq!(parse_hex_pattern("0xFF 0xD8").unwrap(), vec![0xFF, 0xD8]);
    }

    #[test]
    fn parse_hex_comma_separated() {
        assert_eq!(parse_hex_pattern("7F,45,4C,46").unwrap(), vec![0x7F, 0x45, 0x4C, 0x46]);
    }

    #[test]
    fn parse_hex_empty_fails() {
        assert!(parse_hex_pattern("").is_err());
        assert!(parse_hex_pattern("   ").is_err());
    }

    #[test]
    fn parse_hex_invalid_byte() {
        assert!(parse_hex_pattern("GG").is_err());
        assert!(parse_hex_pattern("FFF").is_err()); // 3 chars
    }

    #[test]
    fn parse_hex_too_long() {
        let long = (0..17).map(|i| format!("{:02X}", i)).collect::<Vec<_>>().join(" ");
        assert!(parse_hex_pattern(&long).is_err());
    }
}
