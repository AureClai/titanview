//! Workspace system for TitanView.
//!
//! Workspaces provide contextual analysis environments that automatically
//! configure window layouts, active tools, and visualization modes based
//! on the analysis task at hand.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// File extension for workspace files.
pub const WORKSPACE_EXTENSION: &str = "titan-workspace";

/// A workspace configuration defining the analysis environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    /// Workspace identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description of the workspace purpose.
    pub description: String,
    /// Icon/emoji for quick identification.
    pub icon: String,
    /// Window visibility configuration.
    pub windows: WindowConfig,
    /// Minimap visualization mode.
    pub minimap_mode: MinimapMode,
    /// Hilbert visualization mode.
    pub hilbert_mode: HilbertMode,
    /// Prioritized templates (by name).
    pub templates: Vec<String>,
    /// Auto-load scripts on activation.
    pub startup_scripts: Vec<String>,
    /// Signature categories to prioritize.
    pub signature_focus: Vec<String>,
    /// Color theme adjustments.
    pub theme: ThemeConfig,
    /// Keyboard shortcut (1-9 for Ctrl+N).
    pub shortcut: Option<u8>,
}

/// Window visibility configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WindowConfig {
    pub file_info: bool,
    pub search: bool,
    pub signatures: bool,
    pub hilbert: bool,
    pub disasm: bool,
    pub inspector: bool,
    pub histogram: bool,
    pub xrefs: bool,
    pub bookmarks: bool,
    pub script: bool,
    pub minimap: bool,
}

/// Minimap visualization mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MinimapMode {
    /// Entropy heatmap (default).
    #[default]
    Entropy,
    /// Block classification colors.
    Classification,
    /// Combined entropy + classification.
    Combined,
    /// Byte density visualization.
    ByteDensity,
}

/// Hilbert curve visualization mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum HilbertMode {
    /// Entropy coloring.
    #[default]
    Entropy,
    /// Block classification.
    Classification,
    /// Raw byte values.
    ByteValue,
    /// Bit density.
    BitDensity,
}

/// Theme configuration for workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// Accent color for highlights (hex).
    pub accent: String,
    /// Whether to use high contrast mode.
    pub high_contrast: bool,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            accent: "#4A90D9".to_string(),
            high_contrast: false,
        }
    }
}

impl Workspace {
    /// Create a new empty workspace.
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            description: String::new(),
            icon: "📁".to_string(),
            windows: WindowConfig::default(),
            minimap_mode: MinimapMode::default(),
            hilbert_mode: HilbertMode::default(),
            templates: Vec::new(),
            startup_scripts: Vec::new(),
            signature_focus: Vec::new(),
            theme: ThemeConfig::default(),
            shortcut: None,
        }
    }

    /// Save workspace to a file.
    pub fn save(&self, path: &PathBuf) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load workspace from a file.
    pub fn load(path: &PathBuf) -> anyhow::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        let workspace: Workspace = serde_json::from_str(&json)?;
        Ok(workspace)
    }
}

// ============================================================================
// Built-in Workspaces
// ============================================================================

/// Generic analysis workspace (default).
pub fn workspace_generic() -> Workspace {
    Workspace {
        id: "generic".to_string(),
        name: "Generic Analysis".to_string(),
        description: "General-purpose binary analysis with balanced tool visibility.".to_string(),
        icon: "🔍".to_string(),
        windows: WindowConfig {
            file_info: true,
            search: false,
            signatures: false,
            hilbert: false,
            disasm: false,
            inspector: false,
            histogram: false,
            xrefs: false,
            bookmarks: false,
            script: false,
            minimap: true,
        },
        minimap_mode: MinimapMode::Entropy,
        hilbert_mode: HilbertMode::Entropy,
        templates: vec![],
        startup_scripts: vec![],
        signature_focus: vec![],
        theme: ThemeConfig::default(),
        shortcut: Some(1),
    }
}

/// Network packet analysis workspace.
pub fn workspace_network() -> Workspace {
    Workspace {
        id: "network".to_string(),
        name: "Network Analysis".to_string(),
        description: "Optimized for network captures, protocols, and packet inspection.".to_string(),
        icon: "🌐".to_string(),
        windows: WindowConfig {
            file_info: true,
            search: true,
            signatures: true,
            hilbert: false,
            disasm: false,
            inspector: true,  // For protocol structures
            histogram: false,
            xrefs: false,
            bookmarks: true,
            script: false,
            minimap: true,
        },
        minimap_mode: MinimapMode::ByteDensity,
        hilbert_mode: HilbertMode::BitDensity,
        templates: vec![
            "Ethernet Frame".to_string(),
            "IPv4 Header".to_string(),
            "TCP Header".to_string(),
            "UDP Header".to_string(),
            "DNS Header".to_string(),
        ],
        startup_scripts: vec![],
        signature_focus: vec!["network".to_string(), "protocol".to_string()],
        theme: ThemeConfig {
            accent: "#00A8E8".to_string(),  // Network blue
            high_contrast: false,
        },
        shortcut: Some(2),
    }
}

/// Malware forensics workspace.
pub fn workspace_malware() -> Workspace {
    Workspace {
        id: "malware".to_string(),
        name: "Malware Forensics".to_string(),
        description: "Reverse engineering focus with CFG, disassembly, and signature detection.".to_string(),
        icon: "🦠".to_string(),
        windows: WindowConfig {
            file_info: true,
            search: true,
            signatures: true,  // YARA-like detection
            hilbert: true,     // Visual anomaly detection
            disasm: true,      // Code analysis
            inspector: true,   // PE/ELF headers
            histogram: true,   // Byte distribution
            xrefs: true,       // Cross-references
            bookmarks: true,
            script: true,      // Automation
            minimap: true,
        },
        minimap_mode: MinimapMode::Combined,
        hilbert_mode: HilbertMode::Entropy,
        templates: vec![
            "PE DOS Header".to_string(),
            "PE File Header".to_string(),
            "PE Optional Header".to_string(),
            "PE Section Header".to_string(),
            "ELF Header".to_string(),
        ],
        startup_scripts: vec![],
        signature_focus: vec![
            "executable".to_string(),
            "packed".to_string(),
            "encrypted".to_string(),
            "shellcode".to_string(),
        ],
        theme: ThemeConfig {
            accent: "#E53935".to_string(),  // Warning red
            high_contrast: true,
        },
        shortcut: Some(2),
    }
}

/// Data carving and recovery workspace.
pub fn workspace_carving() -> Workspace {
    Workspace {
        id: "carving".to_string(),
        name: "Data Carving".to_string(),
        description: "File recovery with entropy mapping and automated signature scanning.".to_string(),
        icon: "⛏️".to_string(),
        windows: WindowConfig {
            file_info: true,
            search: true,
            signatures: true,  // File type detection
            hilbert: true,     // Visual structure overview
            disasm: false,
            inspector: true,   // File headers
            histogram: true,   // Entropy analysis
            xrefs: false,
            bookmarks: true,   // Mark found files
            script: true,      // Export automation
            minimap: true,
        },
        minimap_mode: MinimapMode::Classification,
        hilbert_mode: HilbertMode::Classification,
        templates: vec![
            "JPEG Header".to_string(),
            "PNG Header".to_string(),
            "PDF Header".to_string(),
            "ZIP Local Header".to_string(),
        ],
        startup_scripts: vec![],
        signature_focus: vec![
            "image".to_string(),
            "document".to_string(),
            "archive".to_string(),
            "multimedia".to_string(),
        ],
        theme: ThemeConfig {
            accent: "#43A047".to_string(),  // Recovery green
            high_contrast: false,
        },
        shortcut: Some(3),
    }
}

/// Firmware analysis workspace.
pub fn workspace_firmware() -> Workspace {
    Workspace {
        id: "firmware".to_string(),
        name: "Firmware Analysis".to_string(),
        description: "Embedded systems analysis with architecture-aware disassembly.".to_string(),
        icon: "🔧".to_string(),
        windows: WindowConfig {
            file_info: true,
            search: true,
            signatures: true,
            hilbert: true,
            disasm: true,
            inspector: true,
            histogram: true,
            xrefs: true,
            bookmarks: true,
            script: true,
            minimap: true,
        },
        minimap_mode: MinimapMode::Combined,
        hilbert_mode: HilbertMode::ByteValue,
        templates: vec![
            "ELF Header".to_string(),
            "ARM Vector Table".to_string(),
        ],
        startup_scripts: vec![],
        signature_focus: vec![
            "firmware".to_string(),
            "bootloader".to_string(),
            "filesystem".to_string(),
        ],
        theme: ThemeConfig {
            accent: "#FF9800".to_string(),  // Hardware orange
            high_contrast: false,
        },
        shortcut: Some(4),
    }
}

/// Cryptographic analysis workspace.
pub fn workspace_crypto() -> Workspace {
    Workspace {
        id: "crypto".to_string(),
        name: "Crypto Analysis".to_string(),
        description: "Encryption detection with entropy visualization and pattern analysis.".to_string(),
        icon: "🔐".to_string(),
        windows: WindowConfig {
            file_info: true,
            search: true,
            signatures: false,
            hilbert: true,     // Entropy patterns
            disasm: false,
            inspector: false,
            histogram: true,   // Byte distribution critical
            xrefs: false,
            bookmarks: true,
            script: true,      // XOR/crypto scripts
            minimap: true,
        },
        minimap_mode: MinimapMode::Entropy,
        hilbert_mode: HilbertMode::Entropy,
        templates: vec![],
        startup_scripts: vec![],
        signature_focus: vec![
            "encrypted".to_string(),
            "compressed".to_string(),
        ],
        theme: ThemeConfig {
            accent: "#7B1FA2".to_string(),  // Crypto purple
            high_contrast: true,
        },
        shortcut: Some(5),
    }
}

// ============================================================================
// Workspace Manager
// ============================================================================

/// Manages available workspaces and the current active workspace.
pub struct WorkspaceManager {
    /// All available workspaces (built-in + custom).
    pub workspaces: Vec<Workspace>,
    /// Index of currently active workspace.
    pub active_index: usize,
    /// Custom workspaces directory.
    pub custom_dir: Option<PathBuf>,
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceManager {
    /// Create a new workspace manager with built-in workspaces.
    pub fn new() -> Self {
        let workspaces = vec![
            workspace_generic(),
            // workspace_network(),  // Disabled - network features not yet implemented
            workspace_malware(),
            workspace_carving(),
            workspace_firmware(),
            workspace_crypto(),
        ];

        Self {
            workspaces,
            active_index: 0,
            custom_dir: None,
        }
    }

    /// Get the currently active workspace.
    pub fn active(&self) -> &Workspace {
        &self.workspaces[self.active_index]
    }

    /// Switch to a workspace by index.
    pub fn switch_to(&mut self, index: usize) -> bool {
        if index < self.workspaces.len() {
            self.active_index = index;
            true
        } else {
            false
        }
    }

    /// Switch to a workspace by ID.
    pub fn switch_to_id(&mut self, id: &str) -> bool {
        if let Some(index) = self.workspaces.iter().position(|w| w.id == id) {
            self.active_index = index;
            true
        } else {
            false
        }
    }

    /// Find workspace by keyboard shortcut (1-9).
    pub fn find_by_shortcut(&self, shortcut: u8) -> Option<usize> {
        self.workspaces.iter().position(|w| w.shortcut == Some(shortcut))
    }

    /// Add a custom workspace.
    pub fn add_workspace(&mut self, workspace: Workspace) {
        // Check for duplicate ID
        if !self.workspaces.iter().any(|w| w.id == workspace.id) {
            self.workspaces.push(workspace);
        }
    }

    /// Load custom workspaces from a directory.
    pub fn load_custom_workspaces(&mut self, dir: &PathBuf) -> anyhow::Result<usize> {
        self.custom_dir = Some(dir.clone());
        let mut count = 0;

        if dir.exists() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some(WORKSPACE_EXTENSION) {
                    if let Ok(workspace) = Workspace::load(&path) {
                        self.add_workspace(workspace);
                        count += 1;
                    }
                }
            }
        }

        Ok(count)
    }

    /// Save a workspace to the custom directory.
    pub fn save_workspace(&self, workspace: &Workspace) -> anyhow::Result<PathBuf> {
        let dir = self.custom_dir.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No custom workspace directory set"))?;

        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{}.{}", workspace.id, WORKSPACE_EXTENSION));
        workspace.save(&path)?;
        Ok(path)
    }

    /// Get workspace names for UI display.
    pub fn workspace_list(&self) -> Vec<(&str, &str, &str)> {
        self.workspaces.iter()
            .map(|w| (w.id.as_str(), w.name.as_str(), w.icon.as_str()))
            .collect()
    }
}
