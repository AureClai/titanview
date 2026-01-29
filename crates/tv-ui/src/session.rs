//! Session persistence for TitanView.
//!
//! Saves and restores the entire workspace state including:
//! - Window positions and visibility
//! - File path and viewport position
//! - Bookmarks and labels
//! - Custom templates
//! - Analysis state (search, disasm, etc.)

use serde::{Serialize, Deserialize};
use std::path::PathBuf;

/// Version of the session file format.
pub const SESSION_VERSION: u32 = 1;

/// File extension for TitanView session files.
pub const SESSION_EXTENSION: &str = "titan";

/// Complete session state that can be saved/loaded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Session file format version.
    pub version: u32,
    /// Session name/description.
    pub name: String,
    /// When the session was last saved.
    pub saved_at: String,
    /// Path to the analyzed file.
    pub file_path: Option<PathBuf>,
    /// Viewport state.
    pub viewport: ViewportState,
    /// Window visibility and positions.
    pub windows: WindowStates,
    /// Bookmarks and labels.
    pub bookmarks: Vec<BookmarkEntry>,
    pub labels: Vec<LabelEntry>,
    /// Custom loaded templates.
    pub custom_templates: Vec<TemplateEntry>,
    /// Inspector state.
    pub inspector: InspectorSessionState,
    /// Search state.
    pub search: SearchSessionState,
    /// Disassembly state.
    pub disasm: DisasmSessionState,
    /// Hilbert visualization state.
    pub hilbert: HilbertSessionState,
    /// Histogram state.
    pub histogram: HistogramSessionState,
    /// Notes/comments about the session.
    pub notes: String,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            version: SESSION_VERSION,
            name: String::new(),
            saved_at: String::new(),
            file_path: None,
            viewport: ViewportState::default(),
            windows: WindowStates::default(),
            bookmarks: Vec::new(),
            labels: Vec::new(),
            custom_templates: Vec::new(),
            inspector: InspectorSessionState::default(),
            search: SearchSessionState::default(),
            disasm: DisasmSessionState::default(),
            hilbert: HilbertSessionState::default(),
            histogram: HistogramSessionState::default(),
            notes: String::new(),
        }
    }
}

/// Viewport position state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ViewportState {
    /// Current offset in the file.
    pub offset: u64,
}

/// Window visibility states.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowStates {
    pub file_info: WindowState,
    pub search: WindowState,
    pub signatures: WindowState,
    pub hilbert: WindowState,
    pub disasm: WindowState,
    pub inspector: WindowState,
    pub histogram: WindowState,
    pub xrefs: WindowState,
    pub bookmarks: WindowState,
    pub minimap: WindowState,
    pub diff: WindowState,
    pub perf: WindowState,
}

impl Default for WindowStates {
    fn default() -> Self {
        Self {
            file_info: WindowState::new(false),
            search: WindowState::new(false),
            signatures: WindowState::new(false),
            hilbert: WindowState::new(false),
            disasm: WindowState::new(false),
            inspector: WindowState::new(false),
            histogram: WindowState::new(false),
            xrefs: WindowState::new(false),
            bookmarks: WindowState::new(false),
            minimap: WindowState::new(true), // Minimap visible by default
            diff: WindowState::new(false),
            perf: WindowState::new(false),
        }
    }
}

/// State for a single window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub visible: bool,
    /// Optional position (x, y).
    pub position: Option<(f32, f32)>,
    /// Optional size (width, height).
    pub size: Option<(f32, f32)>,
}

impl WindowState {
    pub fn new(visible: bool) -> Self {
        Self {
            visible,
            position: None,
            size: None,
        }
    }
}

/// Bookmark entry for serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkEntry {
    pub offset: u64,
    pub name: String,
    pub color: Option<String>,
}

/// Label entry for serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelEntry {
    pub address: u64,
    pub name: String,
    pub label_type: String,
    pub comment: Option<String>,
}

/// Custom template entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateEntry {
    /// Template JSON content.
    pub json: String,
    /// Original file path (if loaded from file).
    pub source_path: Option<PathBuf>,
}

/// Inspector session state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InspectorSessionState {
    /// Index of selected template (including custom).
    pub selected_template_name: Option<String>,
    /// Current inspection offset.
    pub offset: u64,
    /// Whether auto-detect is enabled.
    pub auto_detect: bool,
}

/// Search session state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchSessionState {
    /// Last search query.
    pub query: String,
    /// Search results (offsets).
    pub results: Vec<u64>,
    /// Selected result index.
    pub selected_index: Option<usize>,
}

/// Disassembly session state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DisasmSessionState {
    /// Starting address for disassembly.
    pub address: u64,
    /// Selected architecture.
    pub architecture: String,
    /// Number of instructions to show.
    pub instruction_count: usize,
}

/// Hilbert visualization state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HilbertSessionState {
    /// Visualization mode.
    pub mode: String,
    /// Curve order (size = 2^order).
    pub order: u32,
    /// File offset for visualization.
    pub offset: u64,
}

/// Histogram session state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HistogramSessionState {
    /// Whether to use log scale.
    pub log_scale: bool,
    /// Scope: "full", "viewport", or "selection".
    pub scope: String,
}

impl Session {
    /// Create a new empty session.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a session with a name.
    pub fn with_name(name: &str) -> Self {
        let mut session = Self::default();
        session.name = name.to_string();
        session
    }

    /// Update the saved_at timestamp.
    pub fn update_timestamp(&mut self) {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // Simple ISO-like format
        self.saved_at = format_timestamp(now);
    }

    /// Get the session file path for a given file.
    pub fn session_path_for(file_path: &std::path::Path) -> PathBuf {
        let mut path = file_path.to_path_buf();
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("session");
        path.set_file_name(format!("{}.{}", name, SESSION_EXTENSION));
        path
    }

    /// Save session to a file.
    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize session: {}", e))?;
        std::fs::write(path, json)
            .map_err(|e| format!("Failed to write session file: {}", e))
    }

    /// Load session from a file.
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read session file: {}", e))?;
        let session: Session = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse session file: {}", e))?;

        // Check version compatibility
        if session.version > SESSION_VERSION {
            return Err(format!(
                "Session file version {} is newer than supported version {}",
                session.version, SESSION_VERSION
            ));
        }

        Ok(session)
    }

    /// Check if a session file exists for the given file.
    pub fn exists_for(file_path: &std::path::Path) -> bool {
        Self::session_path_for(file_path).exists()
    }
}

/// Format a Unix timestamp as a readable string.
fn format_timestamp(secs: u64) -> String {
    // Simple formatting without external crate
    let days_since_1970 = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Approximate date calculation (doesn't account for leap years precisely)
    let mut year = 1970;
    let mut remaining_days = days_since_1970;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let days_per_month = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1;
    for days in days_per_month.iter() {
        if remaining_days < *days {
            break;
        }
        remaining_days -= days;
        month += 1;
    }
    let day = remaining_days + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_default() {
        let session = Session::default();
        assert_eq!(session.version, SESSION_VERSION);
        assert!(session.file_path.is_none());
        assert!(session.bookmarks.is_empty());
    }

    #[test]
    fn test_session_roundtrip() {
        let mut session = Session::with_name("Test Session");
        session.file_path = Some(PathBuf::from("/test/file.bin"));
        session.viewport.offset = 0x1000;
        session.windows.search.visible = true;
        session.bookmarks.push(BookmarkEntry {
            offset: 0x100,
            name: "Start".to_string(),
            color: None,
        });
        session.update_timestamp();

        // Serialize
        let json = serde_json::to_string_pretty(&session).unwrap();
        println!("Session JSON:\n{}", json);

        // Deserialize
        let loaded: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.name, "Test Session");
        assert_eq!(loaded.viewport.offset, 0x1000);
        assert!(loaded.windows.search.visible);
        assert_eq!(loaded.bookmarks.len(), 1);
    }

    #[test]
    fn test_session_path() {
        let file_path = PathBuf::from("/path/to/malware.exe");
        let session_path = Session::session_path_for(&file_path);
        assert_eq!(session_path.file_name().unwrap(), "malware.exe.titan");
    }

    #[test]
    fn test_timestamp_format() {
        let ts = format_timestamp(0);
        assert_eq!(ts, "1970-01-01T00:00:00Z");

        // 2024-01-29 roughly
        let ts2 = format_timestamp(1706500000);
        assert!(ts2.starts_with("2024-01-"));
    }
}
