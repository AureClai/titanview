//! Project file management for TitanView.
//!
//! Stores user annotations, bookmarks, labels, and analysis results
//! in a JSON project file alongside the analyzed binary.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A bookmark marking an interesting location in the file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    /// Offset in the file.
    pub offset: u64,
    /// User-provided name/description.
    pub name: String,
    /// Optional color for visual distinction.
    #[serde(default)]
    pub color: Option<String>,
    /// Optional notes.
    #[serde(default)]
    pub notes: String,
    /// Creation timestamp (Unix time).
    #[serde(default)]
    pub created: u64,
}

impl Bookmark {
    pub fn new(offset: u64, name: String) -> Self {
        Self {
            offset,
            name,
            color: None,
            notes: String::new(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }
    }
}

/// A label naming an address or region.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    /// Address being labeled.
    pub address: u64,
    /// User-provided name.
    pub name: String,
    /// Type of label (function, data, string, etc.).
    #[serde(default)]
    pub label_type: LabelType,
    /// Optional size of the labeled region.
    #[serde(default)]
    pub size: Option<u64>,
    /// Optional notes.
    #[serde(default)]
    pub notes: String,
}

impl Label {
    pub fn new(address: u64, name: String) -> Self {
        Self {
            address,
            name,
            label_type: LabelType::Unknown,
            size: None,
            notes: String::new(),
        }
    }
}

/// Type of labeled item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LabelType {
    #[default]
    Unknown,
    Function,
    Data,
    String,
    Code,
    Import,
    Export,
}

impl LabelType {
    pub fn label(&self) -> &'static str {
        match self {
            LabelType::Unknown => "Unknown",
            LabelType::Function => "Function",
            LabelType::Data => "Data",
            LabelType::String => "String",
            LabelType::Code => "Code",
            LabelType::Import => "Import",
            LabelType::Export => "Export",
        }
    }

    pub fn all() -> &'static [LabelType] {
        &[
            LabelType::Unknown,
            LabelType::Function,
            LabelType::Data,
            LabelType::String,
            LabelType::Code,
            LabelType::Import,
            LabelType::Export,
        ]
    }
}

/// A comment attached to an address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    /// Address the comment is attached to.
    pub address: u64,
    /// Comment text.
    pub text: String,
}

/// Project file containing all user annotations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Project {
    /// Project file format version.
    pub version: u32,
    /// Path to the analyzed file (relative or absolute).
    pub file_path: String,
    /// SHA256 hash of the file (for verification).
    #[serde(default)]
    pub file_hash: String,
    /// File size at time of project creation.
    #[serde(default)]
    pub file_size: u64,
    /// Bookmarks in the file.
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
    /// Labels (address -> name mappings).
    #[serde(default)]
    pub labels: Vec<Label>,
    /// Comments attached to addresses.
    #[serde(default)]
    pub comments: Vec<Comment>,
    /// Custom metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Last modified timestamp.
    #[serde(default)]
    pub last_modified: u64,
}

impl Project {
    /// Current project file format version.
    pub const VERSION: u32 = 1;

    /// Create a new empty project for a file.
    pub fn new(file_path: &Path, file_size: u64) -> Self {
        Self {
            version: Self::VERSION,
            file_path: file_path.display().to_string(),
            file_hash: String::new(),
            file_size,
            bookmarks: Vec::new(),
            labels: Vec::new(),
            comments: Vec::new(),
            metadata: HashMap::new(),
            last_modified: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }
    }

    /// Get the default project file path for a given binary.
    pub fn project_path_for(file_path: &Path) -> PathBuf {
        let mut project_path = file_path.to_path_buf();
        let extension = project_path
            .extension()
            .map(|e| format!("{}.tvproj", e.to_string_lossy()))
            .unwrap_or_else(|| "tvproj".to_string());
        project_path.set_extension(extension);
        project_path
    }

    /// Load a project from a JSON file.
    pub fn load(path: &Path) -> Result<Self, ProjectError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ProjectError::IoError(e.to_string()))?;

        let project: Project = serde_json::from_str(&content)
            .map_err(|e| ProjectError::ParseError(e.to_string()))?;

        Ok(project)
    }

    /// Save the project to a JSON file.
    pub fn save(&mut self, path: &Path) -> Result<(), ProjectError> {
        self.last_modified = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| ProjectError::SerializeError(e.to_string()))?;

        std::fs::write(path, content)
            .map_err(|e| ProjectError::IoError(e.to_string()))?;

        Ok(())
    }

    /// Add a bookmark.
    pub fn add_bookmark(&mut self, bookmark: Bookmark) {
        // Remove existing bookmark at same offset
        self.bookmarks.retain(|b| b.offset != bookmark.offset);
        self.bookmarks.push(bookmark);
        self.bookmarks.sort_by_key(|b| b.offset);
    }

    /// Remove a bookmark by offset.
    pub fn remove_bookmark(&mut self, offset: u64) {
        self.bookmarks.retain(|b| b.offset != offset);
    }

    /// Get bookmark at offset.
    pub fn get_bookmark(&self, offset: u64) -> Option<&Bookmark> {
        self.bookmarks.iter().find(|b| b.offset == offset)
    }

    /// Add or update a label.
    pub fn add_label(&mut self, label: Label) {
        // Remove existing label at same address
        self.labels.retain(|l| l.address != label.address);
        self.labels.push(label);
        self.labels.sort_by_key(|l| l.address);
    }

    /// Remove a label by address.
    pub fn remove_label(&mut self, address: u64) {
        self.labels.retain(|l| l.address != address);
    }

    /// Get label at address.
    pub fn get_label(&self, address: u64) -> Option<&Label> {
        self.labels.iter().find(|l| l.address == address)
    }

    /// Get label by name.
    pub fn get_label_by_name(&self, name: &str) -> Option<&Label> {
        self.labels.iter().find(|l| l.name == name)
    }

    /// Add or update a comment.
    pub fn add_comment(&mut self, comment: Comment) {
        self.comments.retain(|c| c.address != comment.address);
        self.comments.push(comment);
        self.comments.sort_by_key(|c| c.address);
    }

    /// Remove a comment by address.
    pub fn remove_comment(&mut self, address: u64) {
        self.comments.retain(|c| c.address != address);
    }

    /// Get comment at address.
    pub fn get_comment(&self, address: u64) -> Option<&Comment> {
        self.comments.iter().find(|c| c.address == address)
    }

    /// Check if project has any user data.
    pub fn is_empty(&self) -> bool {
        self.bookmarks.is_empty() && self.labels.is_empty() && self.comments.is_empty()
    }

    /// Get statistics about the project.
    pub fn stats(&self) -> ProjectStats {
        ProjectStats {
            bookmarks: self.bookmarks.len(),
            labels: self.labels.len(),
            comments: self.comments.len(),
            functions: self.labels.iter().filter(|l| l.label_type == LabelType::Function).count(),
        }
    }
}

/// Statistics about a project.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProjectStats {
    pub bookmarks: usize,
    pub labels: usize,
    pub comments: usize,
    pub functions: usize,
}

/// Errors that can occur during project operations.
#[derive(Debug, Clone)]
pub enum ProjectError {
    IoError(String),
    ParseError(String),
    SerializeError(String),
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectError::IoError(e) => write!(f, "I/O error: {}", e),
            ProjectError::ParseError(e) => write!(f, "Parse error: {}", e),
            ProjectError::SerializeError(e) => write!(f, "Serialize error: {}", e),
        }
    }
}

impl std::error::Error for ProjectError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_project_path() {
        let file = PathBuf::from("/path/to/binary.exe");
        let proj = Project::project_path_for(&file);
        assert_eq!(proj.extension().unwrap(), "tvproj");
    }

    #[test]
    fn test_bookmark_operations() {
        let mut proj = Project::new(Path::new("test.bin"), 1000);

        proj.add_bookmark(Bookmark::new(0x100, "Start".to_string()));
        proj.add_bookmark(Bookmark::new(0x200, "Middle".to_string()));

        assert_eq!(proj.bookmarks.len(), 2);
        assert!(proj.get_bookmark(0x100).is_some());
        assert!(proj.get_bookmark(0x150).is_none());

        proj.remove_bookmark(0x100);
        assert_eq!(proj.bookmarks.len(), 1);
    }

    #[test]
    fn test_label_operations() {
        let mut proj = Project::new(Path::new("test.bin"), 1000);

        let mut label = Label::new(0x1000, "main".to_string());
        label.label_type = LabelType::Function;
        proj.add_label(label);

        assert_eq!(proj.labels.len(), 1);
        assert_eq!(proj.get_label(0x1000).unwrap().name, "main");
        assert_eq!(proj.get_label_by_name("main").unwrap().address, 0x1000);
    }

    #[test]
    fn test_serialize_deserialize() {
        let mut proj = Project::new(Path::new("test.bin"), 1000);
        proj.add_bookmark(Bookmark::new(0x100, "Test".to_string()));
        proj.add_label(Label::new(0x200, "func".to_string()));

        let json = serde_json::to_string(&proj).unwrap();
        let loaded: Project = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.bookmarks.len(), 1);
        assert_eq!(loaded.labels.len(), 1);
    }
}
