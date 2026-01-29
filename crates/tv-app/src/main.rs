use std::path::PathBuf;
use std::sync::mpsc;

use eframe::egui;
use tv_core::{MappedFile, ByteHistogram};
use tv_ui::{
    AppState, HexPanel, MinimapPanel, PerfState, PerfWindow,
    FileInfoWindow, SearchWindow, SignaturesWindow,
    HilbertState, HilbertWindow,
    DisasmState, DisasmWindow,
    InspectorState, StructInspector,
    HistogramState, HistogramWindow,
    XRefsState, XRefsWindow,
    BookmarksState, BookmarksWindow,
    ScriptState, ScriptWindow,
    WorkspaceManager,
    session::{Session, SESSION_EXTENSION},
};

fn main() -> eframe::Result<()> {
    env_logger::init();

    // CLI argument: open file or session directly
    let initial_file: Option<PathBuf> = std::env::args_os().nth(1).map(PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("TitanView")
            .with_inner_size([1024.0, 768.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "TitanView",
        options,
        Box::new(move |_cc| {
            let mut app = TitanViewApp::default();
            if let Some(path) = initial_file {
                // Check if it's a session file
                if path.extension().and_then(|e| e.to_str()) == Some(SESSION_EXTENSION) {
                    app.load_session(path);
                } else {
                    app.open_file(path);
                }
            }
            Ok(Box::new(app))
        }),
    )
}

/// A chunk of entropy results computed by the background GPU thread.
struct EntropyChunk {
    /// Block index offset (where this chunk starts in the global entropy vec).
    start_block: usize,
    /// Entropy values for this chunk.
    values: Vec<f32>,
    /// Total number of blocks expected for the whole file.
    total_blocks: usize,
}

/// A chunk of classification results computed by the background GPU thread.
struct ClassifyChunk {
    start_block: usize,
    values: Vec<u8>,
    total_blocks: usize,
}

/// Result from a pattern search.
struct SearchResult {
    offsets: Vec<u64>,
    duration_ms: f64,
}

/// Progressive chunk from deep scan.
struct DeepScanChunk {
    /// Signatures found in this chunk.
    signatures: Vec<tv_ui::state::SignatureHit>,
    /// Progress: bytes scanned so far.
    bytes_scanned: u64,
    /// Total bytes to scan.
    total_bytes: u64,
    /// Whether this is the final chunk.
    is_final: bool,
    /// Total duration when final (ms).
    duration_ms: Option<f64>,
}

/// Result from Hilbert texture computation.
struct HilbertResult {
    pixels: Vec<u32>,
    duration_ms: f64,
}

/// Result from diff computation.
struct DiffResult {
    offsets: Vec<u64>,
    total_count: u64,
    duration_ms: f64,
}

/// Result from histogram computation.
struct HistogramResult {
    histogram: ByteHistogram,
    file_size: u64,
    offset: u64,
}

/// Convert (x, y) coordinates to Hilbert curve index.
fn xy2d(n: u32, x: u32, y: u32) -> u64 {
    let mut rx: u32;
    let mut ry: u32;
    let mut d: u64 = 0;
    let mut s: u32 = n / 2;
    let mut px = x;
    let mut py = y;

    while s > 0 {
        rx = if (px & s) > 0 { 1 } else { 0 };
        ry = if (py & s) > 0 { 1 } else { 0 };
        d += (s as u64) * (s as u64) * (((3 * rx) ^ ry) as u64);

        // Rotate quadrant
        if ry == 0 {
            if rx == 1 {
                px = s - 1 - px;
                py = s - 1 - py;
            }
            std::mem::swap(&mut px, &mut py);
        }

        s /= 2;
    }

    d
}

struct TitanViewApp {
    state: AppState,
    /// Receiver for progressive entropy results from GPU thread.
    entropy_rx: Option<mpsc::Receiver<EntropyChunk>>,
    /// Whether entropy computation is in progress.
    computing_entropy: bool,
    /// Receiver for progressive classification results from GPU thread.
    classify_rx: Option<mpsc::Receiver<ClassifyChunk>>,
    /// Whether classification computation is in progress.
    computing_classification: bool,
    /// Receiver for search results from GPU thread.
    search_rx: Option<mpsc::Receiver<SearchResult>>,
    /// Receiver for progressive deep scan chunks from GPU thread.
    deep_scan_rx: Option<mpsc::Receiver<DeepScanChunk>>,
    /// Pending file from drag & drop (processed next frame).
    pending_drop: Option<PathBuf>,
    /// Performance monitoring state.
    perf: PerfState,
    // --- Floating window visibility ---
    /// File Info window visible (F1).
    show_file_info: bool,
    /// Search window visible (Ctrl+F).
    show_search: bool,
    /// Signatures window visible (F2).
    show_signatures: bool,
    /// Hilbert window visible (F4).
    show_hilbert: bool,
    /// Disassembly window visible (F5).
    show_disasm: bool,
    /// Structure Inspector window visible (F7).
    show_inspector: bool,
    /// Histogram window visible (F8).
    show_histogram: bool,
    /// XRefs window visible (F9).
    show_xrefs: bool,
    /// Bookmarks window visible (F10).
    show_bookmarks: bool,
    /// Minimap panel visible.
    show_minimap: bool,
    /// Hilbert visualization state.
    hilbert: HilbertState,
    /// Disassembly state.
    disasm: DisasmState,
    /// Structure inspector state.
    inspector: InspectorState,
    /// Histogram state.
    histogram: HistogramState,
    /// XRefs state.
    xrefs: XRefsState,
    /// Bookmarks state.
    bookmarks: BookmarksState,
    /// Script console state.
    script: ScriptState,
    /// Script console visible (F11).
    show_script: bool,
    /// Receiver for diff computation results.
    diff_rx: Option<mpsc::Receiver<DiffResult>>,
    /// Receiver for Hilbert texture computation.
    hilbert_rx: Option<mpsc::Receiver<HilbertResult>>,
    /// Receiver for histogram computation.
    histogram_rx: Option<mpsc::Receiver<HistogramResult>>,
    // --- Session management ---
    /// Current session path (if saved/loaded).
    session_path: Option<PathBuf>,
    /// Whether the session has unsaved changes.
    session_modified: bool,
    /// Status message for session operations.
    session_status: Option<(String, bool)>, // (message, is_error)
    /// Workspace manager for contextual analysis environments.
    workspaces: WorkspaceManager,
}

impl Default for TitanViewApp {
    fn default() -> Self {
        Self {
            state: AppState::default(),
            entropy_rx: None,
            computing_entropy: false,
            classify_rx: None,
            computing_classification: false,
            search_rx: None,
            deep_scan_rx: None,
            pending_drop: None,
            perf: PerfState::default(),
            // Windows hidden by default, except minimap
            show_file_info: false,
            show_search: false,
            show_signatures: false,
            show_hilbert: false,
            show_disasm: false,
            show_inspector: false,
            show_histogram: false,
            show_xrefs: false,
            show_bookmarks: false,
            show_minimap: true,
            hilbert: HilbertState::default(),
            hilbert_rx: None,
            histogram_rx: None,
            disasm: DisasmState::default(),
            inspector: InspectorState::default(),
            histogram: HistogramState::default(),
            xrefs: XRefsState::default(),
            bookmarks: BookmarksState::default(),
            script: ScriptState::new(),
            show_script: false,
            diff_rx: None,
            session_path: None,
            session_modified: false,
            session_status: None,
            workspaces: WorkspaceManager::new(),
        }
    }
}

impl TitanViewApp {
    /// Apply a workspace configuration to the current UI state.
    fn apply_workspace(&mut self, index: usize) {
        if !self.workspaces.switch_to(index) {
            return;
        }

        let ws = self.workspaces.active().clone();

        // Apply window visibility
        self.show_file_info = ws.windows.file_info;
        self.show_search = ws.windows.search;
        self.show_signatures = ws.windows.signatures;
        self.show_hilbert = ws.windows.hilbert;
        self.show_disasm = ws.windows.disasm;
        self.show_inspector = ws.windows.inspector;
        self.show_histogram = ws.windows.histogram;
        self.show_xrefs = ws.windows.xrefs;
        self.show_bookmarks = ws.windows.bookmarks;
        self.show_script = ws.windows.script;
        self.show_minimap = ws.windows.minimap;

        // Apply Hilbert mode
        self.hilbert.mode = match ws.hilbert_mode {
            tv_ui::workspace::HilbertMode::Entropy => tv_ui::HilbertMode::Entropy,
            tv_ui::workspace::HilbertMode::Classification => tv_ui::HilbertMode::Classification,
            tv_ui::workspace::HilbertMode::ByteValue => tv_ui::HilbertMode::ByteValue,
            tv_ui::workspace::HilbertMode::BitDensity => tv_ui::HilbertMode::BitDensity,
        };
        self.hilbert.invalidate();

        // Log workspace change
        log::info!("Switched to workspace: {} ({})", ws.name, ws.id);
        self.session_status = Some((format!("Workspace: {}", ws.name), false));
    }

    /// Reset the app to its initial landing page state.
    fn reset_to_landing(&mut self) {
        // Reset all state to defaults
        self.state = AppState::default();
        self.entropy_rx = None;
        self.computing_entropy = false;
        self.classify_rx = None;
        self.computing_classification = false;
        self.search_rx = None;
        self.deep_scan_rx = None;
        self.pending_drop = None;
        self.perf = PerfState::default();

        // Reset window visibility to defaults
        self.show_file_info = false;
        self.show_search = false;
        self.show_signatures = false;
        self.show_hilbert = false;
        self.show_disasm = false;
        self.show_inspector = false;
        self.show_histogram = false;
        self.show_xrefs = false;
        self.show_bookmarks = false;
        self.show_script = false;
        self.show_minimap = true;

        // Reset analysis state
        self.hilbert = HilbertState::default();
        self.hilbert_rx = None;
        self.disasm = DisasmState::default();
        self.inspector = InspectorState::default();
        self.histogram = HistogramState::default();
        self.xrefs = XRefsState::default();
        self.bookmarks = BookmarksState::default();
        self.script = ScriptState::new();
        self.diff_rx = None;

        // Reset session state
        self.session_path = None;
        self.session_modified = false;
        self.session_status = None;

        // Reset workspace to default
        self.workspaces = WorkspaceManager::new();

        log::info!("Session closed, returned to landing page");
    }
}

impl TitanViewApp {
    fn open_file(&mut self, path: PathBuf) {
        match MappedFile::open(&path) {
            Ok(mapped) => {
                let file_len = mapped.len();
                log::info!("Opened: {} ({} bytes)", path.display(), file_len);
                self.state.viewport = tv_core::ViewPort::new(0, 4096);
                self.state.entropy = None;
                self.state.cached_entropy_stats = None;
                self.computing_entropy = false;
                self.entropy_rx = None;
                self.state.classification = None;
                self.state.cached_class_counts = None;
                self.computing_classification = false;
                self.classify_rx = None;
                self.state.search = tv_ui::state::SearchState::default();
                self.search_rx = None;
                self.state.goto_open = false;
                self.state.signatures = None;
                self.state.deep_scan = tv_ui::state::DeepScanState::default();
                self.deep_scan_rx = None;
                self.disasm.invalidate();
                self.hilbert.invalidate();
                self.histogram.clear();
                self.xrefs.clear();
                self.bookmarks.clear();
                self.state.edit.clear(); // Clear edit mode when opening new file
                self.state.file = Some(tv_ui::state::LoadedFile { path: path.clone(), mapped });

                // Detect signatures in the first 1 MB (fast CPU scan)
                if let Some(ref f) = self.state.file {
                    let scan_len = (1024 * 1024).min(file_len) as u64;
                    let scan_data = f.mapped.slice(tv_core::FileRegion::new(0, scan_len));
                    let hits = tv_core::signatures::detect_signatures(scan_data, scan_data.len());
                    let sig_hits: Vec<tv_ui::state::SignatureHit> = hits.into_iter().map(|h| {
                        tv_ui::state::SignatureHit {
                            offset: h.offset,
                            name: h.name.to_string(),
                            magic: scan_data[h.offset as usize..(h.offset as usize + h.magic_len).min(scan_data.len())].to_vec(),
                        }
                    }).collect();
                    log::info!("Detected {} signatures", sig_hits.len());
                    self.state.signatures = if sig_hits.is_empty() { None } else { Some(sig_hits) };
                }

                // Launch background entropy computation
                self.launch_entropy_compute(&path, file_len);

                // Check for existing session file and offer to load
                if Session::exists_for(&path) {
                    log::info!("Session file found for {}", path.display());
                    // Auto-load will happen if user explicitly opens .titan file
                }
            }
            Err(e) => {
                log::error!("Failed to open file: {}", e);
            }
        }
    }

    /// Capture current workspace state into a Session.
    fn capture_session(&self) -> Session {
        use tv_ui::session::*;

        let mut session = Session::new();
        session.update_timestamp();

        // File path
        if let Some(ref file) = self.state.file {
            session.file_path = Some(file.path.clone());
        }

        // Viewport
        session.viewport.offset = self.state.viewport.start;

        // Window visibility
        session.windows = WindowStates {
            file_info: WindowState::new(self.show_file_info),
            search: WindowState::new(self.show_search),
            signatures: WindowState::new(self.show_signatures),
            hilbert: WindowState::new(self.show_hilbert),
            disasm: WindowState::new(self.show_disasm),
            inspector: WindowState::new(self.show_inspector),
            histogram: WindowState::new(self.show_histogram),
            xrefs: WindowState::new(self.show_xrefs),
            bookmarks: WindowState::new(self.show_bookmarks),
            minimap: WindowState::new(self.show_minimap),
            diff: WindowState::new(self.state.diff.active),
            perf: WindowState::new(false), // Perf window is transient
        };

        // Bookmarks and labels from project
        if let Some(ref project) = self.bookmarks.project {
            session.bookmarks = project.bookmarks.iter().map(|b| {
                BookmarkEntry {
                    offset: b.offset,
                    name: b.name.clone(),
                    color: None,
                }
            }).collect();

            session.labels = project.labels.iter().map(|l| {
                LabelEntry {
                    address: l.address,
                    name: l.name.clone(),
                    label_type: l.label_type.label().to_string(),
                    comment: if l.notes.is_empty() { None } else { Some(l.notes.clone()) },
                }
            }).collect();
        }

        // Custom templates (skip builtin ones)
        for template in self.inspector.templates.iter().skip(self.inspector.builtin_count) {
            if let Ok(json) = tv_core::save_template_to_json(template) {
                session.custom_templates.push(TemplateEntry {
                    json,
                    source_path: None,
                });
            }
        }

        // Inspector state
        if let Some(template) = self.inspector.templates.get(self.inspector.selected_template) {
            session.inspector.selected_template_name = Some(template.name.clone());
        }
        session.inspector.offset = self.inspector.offset;
        session.inspector.auto_detect = self.inspector.auto_detect;

        // Search state
        session.search.query = self.state.search.query_text.clone();
        if let Some(ref results) = self.state.search.results {
            session.search.results = results.clone();
        }
        session.search.selected_index = self.state.search.selected_result;

        // Disasm state
        session.disasm.address = self.state.viewport.start; // Use viewport offset
        session.disasm.architecture = format!("{:?}", self.disasm.arch);
        session.disasm.instruction_count = self.disasm.max_instructions;

        // Hilbert state
        session.hilbert.mode = format!("{:?}", self.hilbert.mode);
        session.hilbert.order = 9; // Default order
        session.hilbert.offset = self.state.viewport.start;

        // Histogram state
        session.histogram.log_scale = self.histogram.log_scale;
        session.histogram.scope = format!("{:?}", self.histogram.scope);

        session
    }

    /// Restore workspace state from a Session.
    fn restore_session(&mut self, session: &Session) {
        // Open the file if specified
        if let Some(ref file_path) = session.file_path {
            if file_path.exists() {
                self.open_file(file_path.clone());
            } else {
                log::warn!("Session file not found: {}", file_path.display());
                self.session_status = Some((format!("File not found: {}", file_path.display()), true));
                return;
            }
        }

        // Restore viewport
        self.state.viewport.start = session.viewport.offset;

        // Restore window visibility
        self.show_file_info = session.windows.file_info.visible;
        self.show_search = session.windows.search.visible;
        self.show_signatures = session.windows.signatures.visible;
        self.show_hilbert = session.windows.hilbert.visible;
        self.show_disasm = session.windows.disasm.visible;
        self.show_inspector = session.windows.inspector.visible;
        self.show_histogram = session.windows.histogram.visible;
        self.show_xrefs = session.windows.xrefs.visible;
        self.show_bookmarks = session.windows.bookmarks.visible;
        self.show_minimap = session.windows.minimap.visible;
        self.state.diff.active = session.windows.diff.visible;

        // Restore bookmarks and labels
        if !session.bookmarks.is_empty() || !session.labels.is_empty() {
            if let Some(ref file) = self.state.file {
                self.bookmarks.ensure_project(&file.path, file.mapped.len());
            }
            if let Some(ref mut project) = self.bookmarks.project {
                for bookmark in &session.bookmarks {
                    project.add_bookmark(tv_core::Bookmark::new(bookmark.offset, bookmark.name.clone()));
                }
                for label in &session.labels {
                    let label_type = match label.label_type.as_str() {
                        "func" => tv_core::LabelType::Function,
                        "data" => tv_core::LabelType::Data,
                        "str" => tv_core::LabelType::String,
                        "code" => tv_core::LabelType::Code,
                        "import" => tv_core::LabelType::Import,
                        "export" => tv_core::LabelType::Export,
                        _ => tv_core::LabelType::Unknown,
                    };
                    let mut l = tv_core::Label::new(label.address, label.name.clone());
                    l.label_type = label_type;
                    l.notes = label.comment.clone().unwrap_or_default();
                    project.add_label(l);
                }
            }
        }

        // Restore custom templates
        for entry in &session.custom_templates {
            if let Err(e) = self.inspector.add_template_from_json(&entry.json) {
                log::warn!("Failed to load custom template: {}", e);
            }
        }

        // Restore inspector state
        if let Some(ref name) = session.inspector.selected_template_name {
            if let Some(idx) = self.inspector.templates.iter().position(|t| &t.name == name) {
                self.inspector.selected_template = idx;
            }
        }
        self.inspector.offset = session.inspector.offset;
        self.inspector.offset_text = format!("0x{:X}", session.inspector.offset);
        self.inspector.auto_detect = session.inspector.auto_detect;

        // Restore search state
        self.state.search.query_text = session.search.query.clone();
        if !session.search.results.is_empty() {
            self.state.search.results = Some(session.search.results.clone());
            self.state.search.selected_result = session.search.selected_index;
            self.state.search.rebuild_highlights();
        }

        // Restore disasm state
        self.disasm.max_instructions = session.disasm.instruction_count;
        // Architecture is auto-detected, don't override

        // Restore hilbert state
        // Mode is restored via format string match
        self.hilbert.texture_size = 512; // Default size

        // Restore histogram state
        self.histogram.log_scale = session.histogram.log_scale;

        self.session_modified = false;
        log::info!("Session restored: {} bookmarks, {} labels, {} custom templates",
            session.bookmarks.len(), session.labels.len(), session.custom_templates.len());
    }

    /// Save session to the current path or prompt for new path.
    fn save_session(&mut self) {
        let session = self.capture_session();

        let path = if let Some(ref p) = self.session_path {
            p.clone()
        } else if let Some(ref file) = self.state.file {
            Session::session_path_for(&file.path)
        } else {
            // Prompt for path
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("TitanView Session", &[SESSION_EXTENSION])
                .set_file_name("session.titan")
                .save_file()
            {
                path
            } else {
                return;
            }
        };

        match session.save(&path) {
            Ok(()) => {
                self.session_path = Some(path.clone());
                self.session_modified = false;
                self.session_status = Some((format!("Session saved: {}", path.display()), false));
                log::info!("Session saved to {}", path.display());
            }
            Err(e) => {
                self.session_status = Some((format!("Save failed: {}", e), true));
                log::error!("Failed to save session: {}", e);
            }
        }
    }

    /// Save session to a new path.
    fn save_session_as(&mut self) {
        let session = self.capture_session();

        if let Some(path) = rfd::FileDialog::new()
            .add_filter("TitanView Session", &[SESSION_EXTENSION])
            .set_file_name("session.titan")
            .save_file()
        {
            match session.save(&path) {
                Ok(()) => {
                    self.session_path = Some(path.clone());
                    self.session_modified = false;
                    self.session_status = Some((format!("Session saved: {}", path.display()), false));
                    log::info!("Session saved to {}", path.display());
                }
                Err(e) => {
                    self.session_status = Some((format!("Save failed: {}", e), true));
                    log::error!("Failed to save session: {}", e);
                }
            }
        }
    }

    /// Load session from a file.
    fn load_session(&mut self, path: PathBuf) {
        match Session::load(&path) {
            Ok(session) => {
                self.restore_session(&session);
                self.session_path = Some(path.clone());
                self.session_status = Some((format!("Session loaded: {}", path.display()), false));
            }
            Err(e) => {
                self.session_status = Some((format!("Load failed: {}", e), true));
                log::error!("Failed to load session: {}", e);
            }
        }
    }

    fn launch_entropy_compute(&mut self, path: &PathBuf, file_len: u64) {
        if file_len == 0 {
            self.state.entropy = Some(vec![]);
            self.state.classification = Some(vec![]);
            return;
        }

        let (entropy_tx, entropy_rx) = mpsc::channel();
        let (classify_tx, classify_rx) = mpsc::channel();
        self.entropy_rx = Some(entropy_rx);
        self.classify_rx = Some(classify_rx);
        self.computing_entropy = true;
        self.computing_classification = true;

        let path = path.clone();

        std::thread::spawn(move || {
            // Init GPU on this thread
            let ctx = match pollster::block_on(tv_gpu::GpuContext::new()) {
                Ok(ctx) => ctx,
                Err(e) => {
                    log::error!("GPU init failed: {}", e);
                    return;
                }
            };

            // Open a separate mmap for this thread (MappedFile is not Send)
            let file = match MappedFile::open(&path) {
                Ok(f) => f,
                Err(e) => {
                    log::error!("Failed to reopen file for GPU: {}", e);
                    return;
                }
            };

            // Adaptive block size based on file size:
            // - Small files (<64MB): 256 bytes (high resolution)
            // - Medium files (<1GB): 1KB
            // - Large files (<4GB): 4KB
            // - Very large files (>4GB): 16KB
            // This keeps total blocks under ~1M for reasonable performance
            let block_size: u64 = if file_len < 64 * 1024 * 1024 {
                256
            } else if file_len < 1024 * 1024 * 1024 {
                1024
            } else if file_len < 4 * 1024 * 1024 * 1024 {
                4096
            } else {
                16384
            };
            let total_blocks = file_len.div_ceil(block_size) as usize;
            log::info!("Using block size {} for {} blocks", block_size, total_blocks);

            // Process in chunks of ~64 MB to report progress (larger chunks = fewer GPU dispatches)
            let bytes_per_chunk: u64 = 64 * 1024 * 1024;
            let mut offset: u64 = 0;
            let mut block_offset: usize = 0;

            while offset < file_len {
                let chunk_len = bytes_per_chunk.min(file_len - offset);
                let chunk_data = file.slice(tv_core::FileRegion::new(offset, chunk_len));

                // Dispatch 1: Entropy
                match ctx.compute_entropy(chunk_data, block_size as u32) {
                    Ok(values) => {
                        let num_values = values.len();
                        if entropy_tx.send(EntropyChunk {
                            start_block: block_offset,
                            values,
                            total_blocks,
                        }).is_err() {
                            return;
                        }

                        // Dispatch 2: Classification (same chunk)
                        match ctx.compute_classification(chunk_data, block_size as u32) {
                            Ok(classes) => {
                                if classify_tx.send(ClassifyChunk {
                                    start_block: block_offset,
                                    values: classes,
                                    total_blocks,
                                }).is_err() {
                                    return;
                                }
                            }
                            Err(e) => {
                                log::error!("GPU classification failed at offset {}: {}", offset, e);
                                return;
                            }
                        }

                        block_offset += num_values;
                    }
                    Err(e) => {
                        log::error!("GPU entropy failed at offset {}: {}", offset, e);
                        return;
                    }
                }

                offset += chunk_len;
            }

            log::info!("Entropy + classification complete: {} blocks", block_offset);
        });
    }

    /// Launch a background GPU pattern search in file-level chunks (64 MB).
    fn launch_search(&mut self) {
        let pattern = match &self.state.search.pattern {
            Some(p) => p.clone(),
            None => return,
        };
        let path = match &self.state.file {
            Some(f) => f.path.clone(),
            None => return,
        };
        let file_len = self.state.file_len();

        let (tx, rx) = mpsc::channel();
        self.search_rx = Some(rx);

        std::thread::spawn(move || {
            let start_time = std::time::Instant::now();

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                Self::run_search(path, file_len, pattern)
            }));

            let duration_ms = start_time.elapsed().as_secs_f64() * 1000.0;

            let offsets = match result {
                Ok(Ok(offsets)) => offsets,
                Ok(Err(e)) => {
                    log::error!("Pattern search failed: {}", e);
                    vec![]
                }
                Err(_) => {
                    log::error!("Pattern search panicked");
                    vec![]
                }
            };

            log::info!("Parallel CPU search: {} matches in {:.1}ms", offsets.len(), duration_ms);
            let _ = tx.send(SearchResult { offsets, duration_ms });
        });
    }

    /// Run the actual search using parallel CPU scanner (SIMD + rayon).
    /// This is 5-20x faster than GPU for single patterns due to no PCIe overhead.
    fn run_search(path: PathBuf, _file_len: u64, pattern: Vec<u8>) -> anyhow::Result<Vec<u64>> {
        let file = MappedFile::open(&path)
            .map_err(|e| anyhow::anyhow!("Failed to reopen file for search: {}", e))?;

        // Get the full file as a slice and run parallel SIMD search
        let data = file.slice(tv_core::FileRegion::new(0, file.len()));
        let offsets = tv_core::scan_pattern_parallel(data, &pattern);

        Ok(offsets)
    }

    /// Poll search results channel.
    fn poll_search(&mut self) {
        let rx = match &self.search_rx {
            Some(rx) => rx,
            None => return,
        };

        match rx.try_recv() {
            Ok(result) => {
                self.state.search.search_duration_ms = Some(result.duration_ms);
                self.state.search.results = Some(result.offsets);
                self.state.search.searching = false;
                self.state.search.rebuild_highlights();
                self.search_rx = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.state.search.searching = false;
                self.search_rx = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    /// Poll the classification channel and accumulate results.
    fn poll_classification(&mut self) {
        let rx = match &self.classify_rx {
            Some(rx) => rx,
            None => return,
        };

        let mut got_any = false;
        while let Ok(chunk) = rx.try_recv() {
            got_any = true;
            let classification = self.state.classification.get_or_insert_with(|| {
                vec![0u8; chunk.total_blocks]
            });

            if classification.len() < chunk.total_blocks {
                classification.resize(chunk.total_blocks, 3); // default Binary
            }

            let end = (chunk.start_block + chunk.values.len()).min(classification.len());
            classification[chunk.start_block..end]
                .copy_from_slice(&chunk.values[..end - chunk.start_block]);
        }

        // Invalidate minimap cache when new classification data arrives
        if got_any {
            self.state.minimap_cache.invalidate();
        }

        if !got_any && self.computing_classification {
            match rx.try_recv() {
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.computing_classification = false;
                    log::info!("Classification computation finished");
                    // Cache classification counts to avoid recomputing every frame
                    if let Some(ref classification) = self.state.classification {
                        let mut counts = [0u32; 5];
                        for &c in classification.iter() {
                            counts[(c as usize).min(4)] += 1;
                        }
                        self.state.cached_class_counts = Some(counts);
                    }
                }
                _ => {}
            }
        }
    }

    /// Poll the entropy channel and accumulate results.
    fn poll_entropy(&mut self) {
        let rx = match &self.entropy_rx {
            Some(rx) => rx,
            None => return,
        };

        // Drain all available results
        let mut got_any = false;
        while let Ok(chunk) = rx.try_recv() {
            got_any = true;
            let entropy = self.state.entropy.get_or_insert_with(|| {
                vec![0.0f32; chunk.total_blocks]
            });

            // Ensure vec is large enough
            if entropy.len() < chunk.total_blocks {
                entropy.resize(chunk.total_blocks, 0.0);
            }

            let end = (chunk.start_block + chunk.values.len()).min(entropy.len());
            entropy[chunk.start_block..end]
                .copy_from_slice(&chunk.values[..end - chunk.start_block]);
        }

        // Invalidate minimap cache when new entropy data arrives
        if got_any {
            self.state.minimap_cache.invalidate();
        }

        // Check if channel is closed (computation done)
        if !got_any && self.computing_entropy {
            // Try a non-blocking recv to check if closed
            match rx.try_recv() {
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.computing_entropy = false;
                    log::info!("Entropy computation finished");
                    // Cache entropy stats to avoid recomputing every frame
                    if let Some(ref entropy) = self.state.entropy {
                        if !entropy.is_empty() {
                            let sum: f32 = entropy.iter().sum();
                            let avg = sum / entropy.len() as f32;
                            self.state.cached_entropy_stats = Some(tv_ui::state::EntropyStats {
                                avg,
                                block_count: entropy.len(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Launch GPU deep scan (multi-pattern signature detection on full file).
    /// Processes file in 64MB chunks and streams results progressively.
    fn launch_deep_scan(&mut self) {
        let path = match &self.state.file {
            Some(f) => f.path.clone(),
            None => return,
        };
        let file_len = self.state.file_len();

        // Initialize progress tracking
        self.state.deep_scan.bytes_scanned = 0;
        self.state.deep_scan.total_bytes = file_len;
        self.state.deep_scan.results = Some(Vec::new()); // Start with empty vec

        let (tx, rx) = mpsc::channel();
        self.deep_scan_rx = Some(rx);

        std::thread::spawn(move || {
            let start_time = std::time::Instant::now();

            // Init GPU on worker thread
            let ctx = match pollster::block_on(tv_gpu::GpuContext::new()) {
                Ok(ctx) => ctx,
                Err(e) => {
                    log::error!("GPU init for deep scan failed: {}", e);
                    let _ = tx.send(DeepScanChunk {
                        signatures: vec![],
                        bytes_scanned: file_len,
                        total_bytes: file_len,
                        is_final: true,
                        duration_ms: Some(start_time.elapsed().as_secs_f64() * 1000.0),
                    });
                    return;
                }
            };

            let file = match MappedFile::open(&path) {
                Ok(f) => f,
                Err(e) => {
                    log::error!("Failed to reopen file for deep scan: {}", e);
                    let _ = tx.send(DeepScanChunk {
                        signatures: vec![],
                        bytes_scanned: file_len,
                        total_bytes: file_len,
                        is_final: true,
                        duration_ms: Some(start_time.elapsed().as_secs_f64() * 1000.0),
                    });
                    return;
                }
            };

            // Build pattern list from all signatures
            let signatures = tv_core::signatures::SIGNATURES;
            let patterns: Vec<&[u8]> = signatures.iter().map(|s| s.magic).collect();
            let max_pattern_len = patterns.iter().map(|p| p.len()).max().unwrap_or(0) as u64;

            // Process in 64MB chunks for progressive results and lower memory
            const CHUNK_SIZE: u64 = 64 * 1024 * 1024;
            let mut offset: u64 = 0;
            let mut total_found = 0usize;

            while offset < file_len {
                // Overlap chunks by max pattern length to catch matches at boundaries
                let overlap = if offset > 0 { max_pattern_len.saturating_sub(1) } else { 0 };
                let chunk_start = offset.saturating_sub(overlap);
                let chunk_len = CHUNK_SIZE.min(file_len - chunk_start);
                let chunk_data = file.slice(tv_core::FileRegion::new(chunk_start, chunk_len));

                // Run GPU multi-pattern scan on this chunk
                let chunk_matches = match ctx.scan_multi_pattern(chunk_data, &patterns) {
                    Ok(m) => m,
                    Err(e) => {
                        log::error!("Deep scan chunk failed at offset {}: {}", offset, e);
                        vec![]
                    }
                };

                // Convert matches to SignatureHits, adjusting offsets for chunk position
                // Filter out duplicates from overlap region (only keep matches >= offset)
                let chunk_sigs: Vec<tv_ui::state::SignatureHit> = chunk_matches
                    .into_iter()
                    .filter_map(|m| {
                        let absolute_offset = chunk_start + m.offset;
                        // Only include if this match starts at or after our non-overlap region
                        if absolute_offset >= offset || offset == 0 {
                            let sig = &signatures[m.pattern_idx as usize];
                            Some(tv_ui::state::SignatureHit {
                                offset: absolute_offset,
                                name: sig.name.to_string(),
                                magic: sig.magic.to_vec(),
                            })
                        } else {
                            None // Duplicate from overlap
                        }
                    })
                    .collect();

                total_found += chunk_sigs.len();
                let bytes_done = (offset + CHUNK_SIZE).min(file_len);
                let is_final = bytes_done >= file_len;

                // Send chunk results
                if tx.send(DeepScanChunk {
                    signatures: chunk_sigs,
                    bytes_scanned: bytes_done,
                    total_bytes: file_len,
                    is_final,
                    duration_ms: if is_final {
                        Some(start_time.elapsed().as_secs_f64() * 1000.0)
                    } else {
                        None
                    },
                }).is_err() {
                    return; // Receiver dropped
                }

                offset += CHUNK_SIZE;
            }

            log::info!("GPU deep scan complete: {} signatures", total_found);
        });
    }

    /// Launch Hilbert texture computation in background.
    fn launch_hilbert_compute(&mut self) {
        let path = match &self.state.file {
            Some(f) => f.path.clone(),
            None => return,
        };
        let file_len = self.state.file_len();
        let texture_size = self.hilbert.texture_size;
        let mode = self.hilbert.mode.as_u32();

        // Clone entropy and classification data if available
        let entropy = self.state.entropy.clone();
        let classification = self.state.classification.clone();

        let (tx, rx) = mpsc::channel();
        self.hilbert_rx = Some(rx);

        std::thread::spawn(move || {
            let start_time = std::time::Instant::now();

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // Init GPU
                let ctx = pollster::block_on(tv_gpu::GpuContext::new())
                    .map_err(|e| anyhow::anyhow!("GPU init failed: {}", e))?;

                // Pre-sample bytes for Byte Value (mode 2) or Bit Density (mode 3) using Hilbert mapping
                let sampled_bytes = if mode == 2 || mode == 3 {
                    let file = MappedFile::open(&path)
                        .map_err(|e| anyhow::anyhow!("Failed to open file: {}", e))?;

                    let total_pixels = (texture_size * texture_size) as u64;

                    if mode == 3 {
                        // Bit density mode: each pixel = 1 bit
                        // Sample bits from file (with downsampling if file is large)
                        let bits_per_file_bit = (file_len * 8 / total_pixels).max(1);

                        let mut samples = vec![0u8; total_pixels as usize];
                        for pixel_idx in 0..total_pixels as usize {
                            // Map pixel to bit in file (with downsampling if needed)
                            let file_bit_idx = (pixel_idx as u64) * bits_per_file_bit;
                            let file_byte_idx = file_bit_idx / 8;
                            let bit_in_byte = (file_bit_idx % 8) as u8;

                            if file_byte_idx < file_len {
                                let byte_val = file.slice(tv_core::FileRegion::new(file_byte_idx, 1))
                                    .first()
                                    .copied()
                                    .unwrap_or(0);
                                // Extract the specific bit and store in sample
                                // Pack bits into bytes for the shader
                                let byte_out_idx = pixel_idx / 8;
                                let bit_out_pos = 7 - (pixel_idx % 8);
                                let bit = (byte_val >> (7 - bit_in_byte)) & 1;
                                samples[byte_out_idx] |= bit << bit_out_pos;
                            }
                        }
                        Some(samples)
                    } else {
                        // Byte value mode: each pixel = 1 byte sample
                        let bytes_per_pixel = (file_len / total_pixels).max(1);

                        // Sample one byte per pixel, ordered by pixel index (y * width + x)
                        let mut samples = vec![0u8; total_pixels as usize];
                        for y in 0..texture_size {
                            for x in 0..texture_size {
                                let pixel_idx = (y * texture_size + x) as usize;
                                let hilbert_idx = xy2d(texture_size, x, y);
                                let file_offset = (hilbert_idx as u64) * bytes_per_pixel;

                                if file_offset < file_len {
                                    samples[pixel_idx] = file.slice(tv_core::FileRegion::new(file_offset, 1))
                                        .first()
                                        .copied()
                                        .unwrap_or(0);
                                }
                            }
                        }
                        Some(samples)
                    }
                } else {
                    None
                };

                ctx.compute_hilbert_texture(
                    file_len,
                    entropy.as_deref(),
                    classification.as_deref(),
                    sampled_bytes.as_deref(),
                    texture_size,
                    mode,
                )
            }));

            let duration_ms = start_time.elapsed().as_secs_f64() * 1000.0;

            let pixels = match result {
                Ok(Ok(p)) => p,
                Ok(Err(e)) => {
                    log::error!("Hilbert computation failed: {}", e);
                    vec![]
                }
                Err(_) => {
                    log::error!("Hilbert computation panicked");
                    vec![]
                }
            };

            log::info!("Hilbert texture computed: {}x{} in {:.1}ms", texture_size, texture_size, duration_ms);
            let _ = tx.send(HilbertResult { pixels, duration_ms });
        });
    }

    /// Poll Hilbert computation results.
    fn poll_hilbert(&mut self) {
        let rx = match &self.hilbert_rx {
            Some(rx) => rx,
            None => return,
        };

        match rx.try_recv() {
            Ok(result) => {
                self.hilbert.pending_pixels = Some(result.pixels);
                self.hilbert.compute_time_ms = Some(result.duration_ms);
                self.hilbert_rx = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.hilbert.computing = false;
                self.hilbert_rx = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    /// Launch diff computation on GPU.
    fn launch_diff_compute(&mut self) {
        let file_a = match &self.state.file {
            Some(f) => f.mapped.slice(tv_core::FileRegion::new(0, f.mapped.len())).to_vec(),
            None => return,
        };
        let file_b = match &self.state.diff.file_b {
            Some(f) => f.mapped.slice(tv_core::FileRegion::new(0, f.mapped.len())).to_vec(),
            None => return,
        };

        let (tx, rx) = mpsc::channel();
        self.diff_rx = Some(rx);

        std::thread::spawn(move || {
            let start_time = std::time::Instant::now();

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let ctx = pollster::block_on(tv_gpu::GpuContext::new())
                    .map_err(|e| anyhow::anyhow!("GPU init failed: {}", e))?;

                // Compute diff with GPU, limit to 100k differences
                ctx.compute_diff(&file_a, &file_b, 100_000)
            }));

            let duration_ms = start_time.elapsed().as_secs_f64() * 1000.0;

            let (offsets, total_count) = match result {
                Ok(Ok(diffs)) => {
                    let count = diffs.len() as u64;
                    (diffs, count)
                }
                Ok(Err(e)) => {
                    log::error!("Diff computation failed: {}", e);
                    (vec![], 0)
                }
                Err(_) => {
                    log::error!("Diff computation panicked");
                    (vec![], 0)
                }
            };

            log::info!("Diff computed: {} differences in {:.1} ms", total_count, duration_ms);
            let _ = tx.send(DiffResult { offsets, total_count, duration_ms });
        });
    }

    /// Poll diff computation results.
    fn poll_diff(&mut self) {
        let rx = match &self.diff_rx {
            Some(rx) => rx,
            None => return,
        };

        match rx.try_recv() {
            Ok(result) => {
                self.state.diff.diff_offsets = Some(result.offsets);
                self.state.diff.diff_count = result.total_count;
                self.state.diff.compute_time_ms = Some(result.duration_ms);
                self.state.diff.computing = false;
                self.state.diff.selected_diff = if result.total_count > 0 { Some(0) } else { None };
                self.diff_rx = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.state.diff.computing = false;
                self.diff_rx = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    /// Launch histogram computation in background thread.
    fn launch_histogram(&mut self) {
        let file = match &self.state.file {
            Some(f) => f,
            None => {
                self.histogram.computing = false;
                return;
            }
        };

        let file_len = file.mapped.len();
        let path = file.path.clone();

        // Determine what region to analyze based on scope
        let (start, len) = match self.histogram.scope {
            tv_ui::HistogramScope::FullFile => {
                (0u64, file_len.min(self.histogram.max_bytes as u64))
            }
            tv_ui::HistogramScope::Viewport => {
                let vp_start = self.state.viewport.start;
                let vp_size = (self.state.viewport.visible_bytes as u64)
                    .min(file_len - vp_start.min(file_len));
                (vp_start, vp_size.min(self.histogram.max_bytes as u64))
            }
            tv_ui::HistogramScope::Selection => {
                (0u64, file_len.min(self.histogram.max_bytes as u64))
            }
        };

        let cached_file_size = self.histogram.cached_file_size();
        let cached_offset = self.histogram.cached_offset();

        let (tx, rx) = mpsc::channel();
        self.histogram_rx = Some(rx);

        std::thread::spawn(move || {
            let file = match MappedFile::open(&path) {
                Ok(f) => f,
                Err(_) => return,
            };

            let data = file.slice(tv_core::FileRegion::new(start, len));
            let histogram = ByteHistogram::from_data(data);

            let _ = tx.send(HistogramResult {
                histogram,
                file_size: cached_file_size,
                offset: cached_offset,
            });
        });
    }

    /// Poll histogram computation results.
    fn poll_histogram(&mut self) {
        let rx = match &self.histogram_rx {
            Some(rx) => rx,
            None => return,
        };

        match rx.try_recv() {
            Ok(result) => {
                self.histogram.set_result(result.histogram, result.file_size, result.offset);
                self.histogram_rx = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.histogram.computing = false;
                self.histogram_rx = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    /// Poll deep scan results channel and accumulate chunks progressively.
    fn poll_deep_scan(&mut self) {
        let rx = match &self.deep_scan_rx {
            Some(rx) => rx,
            None => return,
        };

        // Drain all available chunks
        loop {
            match rx.try_recv() {
                Ok(chunk) => {
                    // Update progress
                    self.state.deep_scan.bytes_scanned = chunk.bytes_scanned;
                    self.state.deep_scan.total_bytes = chunk.total_bytes;

                    // Accumulate signatures
                    if !chunk.signatures.is_empty() {
                        let results = self.state.deep_scan.results.get_or_insert_with(Vec::new);
                        results.extend(chunk.signatures);
                    }

                    // Check if final chunk
                    if chunk.is_final {
                        self.state.deep_scan.duration_ms = chunk.duration_ms;
                        self.state.deep_scan.scanning = false;
                        self.deep_scan_rx = None;
                        log::info!(
                            "Deep scan finished: {} signatures in {:.1}ms",
                            self.state.deep_scan.results.as_ref().map_or(0, |r| r.len()),
                            chunk.duration_ms.unwrap_or(0.0)
                        );
                        return;
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    // Channel closed unexpectedly
                    self.state.deep_scan.scanning = false;
                    self.deep_scan_rx = None;
                    return;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // No more chunks available right now
                    return;
                }
            }
        }
    }

    /// Show split view for binary diff comparison.
    fn show_diff_split_view(ui: &mut egui::Ui, state: &mut AppState) {
        // Diff toolbar
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Diff Mode").strong());
            ui.separator();

            // File A info
            ui.label(egui::RichText::new("A:").strong());
            ui.label(state.file_name());

            ui.separator();

            // File B info
            ui.label(egui::RichText::new("B:").strong());
            ui.label(state.diff.file_b_name());

            ui.separator();

            // Sync scroll toggle
            ui.checkbox(&mut state.diff.sync_scroll, "Sync scroll");

            ui.separator();

            // Compute diff button
            if state.diff.computing {
                ui.spinner();
                ui.label("Computing...");
            } else {
                if ui.button("Compute Diff").clicked() {
                    state.diff.computing = true;
                }

                if let Some(count) = state.diff.diff_offsets.as_ref().map(|v| v.len()) {
                    let total = state.diff.diff_count;
                    if total > count as u64 {
                        ui.label(format!("{} diffs (showing {})", total, count));
                    } else {
                        ui.label(format!("{} diffs", count));
                    }

                    // Navigation
                    let sel = state.diff.selected_diff.unwrap_or(0);
                    if ui.small_button("<").on_hover_text("Previous diff").clicked() && sel > 0 {
                        state.diff.selected_diff = Some(sel - 1);
                        if let Some(offsets) = &state.diff.diff_offsets {
                            if let Some(&offset) = offsets.get(sel - 1) {
                                state.viewport.start = (offset / 16) * 16;
                                state.diff.scroll_offset = 0.0;
                            }
                        }
                    }
                    ui.label(format!("{}/{}", sel + 1, count));
                    if ui.small_button(">").on_hover_text("Next diff").clicked() && sel + 1 < count {
                        state.diff.selected_diff = Some(sel + 1);
                        if let Some(offsets) = &state.diff.diff_offsets {
                            if let Some(&offset) = offsets.get(sel + 1) {
                                state.viewport.start = (offset / 16) * 16;
                                state.diff.scroll_offset = 0.0;
                            }
                        }
                    }
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close Diff").clicked() {
                    state.diff.active = false;
                    state.diff.close_file_b();
                }
                if ui.button("Close B").clicked() {
                    state.diff.close_file_b();
                }
            });
        });

        ui.separator();

        // Split view with synchronized scrolling
        let scroll_offset = state.diff.scroll_offset;
        let mut new_scroll_a = scroll_offset;
        let mut new_scroll_b = scroll_offset;

        ui.columns(2, |columns| {
            // Left panel (File A)
            columns[0].vertical(|ui| {
                ui.label(egui::RichText::new("File A").strong().color(egui::Color32::from_rgb(100, 200, 100)));
                new_scroll_a = HexPanel::show_with_scroll(ui, state, scroll_offset);
            });

            // Right panel (File B)
            columns[1].vertical(|ui| {
                ui.label(egui::RichText::new("File B").strong().color(egui::Color32::from_rgb(200, 100, 100)));
                new_scroll_b = HexPanel::show_file_b(ui, state, scroll_offset);
            });
        });

        // Sync scroll: use whichever panel was scrolled
        if state.diff.sync_scroll {
            let delta_a = (new_scroll_a - scroll_offset).abs();
            let delta_b = (new_scroll_b - scroll_offset).abs();
            state.diff.scroll_offset = if delta_a > delta_b { new_scroll_a } else { new_scroll_b };
        }
    }
}

impl eframe::App for TitanViewApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Update performance metrics
        self.perf.begin_frame();

        // Handle drag & drop
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                if let Some(path) = i.raw.dropped_files[0].path.clone() {
                    self.pending_drop = Some(path);
                }
            }
        });
        if let Some(path) = self.pending_drop.take() {
            self.open_file(path);
        }

        // Poll background results
        self.poll_entropy();
        self.poll_classification();
        self.poll_search();
        self.poll_deep_scan();
        self.poll_hilbert();
        self.poll_diff();
        self.poll_histogram();

        // Check if search was requested by the UI
        if self.state.search.searching && self.search_rx.is_none() {
            self.launch_search();
        }

        // Check if histogram computation was requested
        if self.histogram.computing && self.histogram_rx.is_none() {
            self.launch_histogram();
        }

        // Check if deep scan was requested by the UI
        if self.state.deep_scan.scanning && self.deep_scan_rx.is_none() {
            self.launch_deep_scan();
        }

        // Check if Hilbert computation was requested
        if self.hilbert.computing && self.hilbert_rx.is_none() {
            self.launch_hilbert_compute();
        }

        // Check if diff computation was requested
        if self.state.diff.computing && self.diff_rx.is_none() {
            self.launch_diff_compute();
        }

        // Request repaint while computing or when any floating window needs updates
        if self.computing_entropy || self.computing_classification
            || self.state.search.searching || self.state.deep_scan.scanning
            || self.hilbert.computing || self.state.diff.computing
            || self.histogram.computing || self.perf.visible {
            ctx.request_repaint();
        }

        // Handle keyboard shortcuts
        ctx.input(|i| {
            // F1: File Info
            if i.key_pressed(egui::Key::F1) {
                self.show_file_info = !self.show_file_info;
            }
            // F2: Signatures
            if i.key_pressed(egui::Key::F2) {
                self.show_signatures = !self.show_signatures;
            }
            // F3: Performance
            if i.key_pressed(egui::Key::F3) {
                self.perf.visible = !self.perf.visible;
            }
            // F4: Hilbert Curve
            if i.key_pressed(egui::Key::F4) {
                self.show_hilbert = !self.show_hilbert;
            }
            // F5: Disassembly
            if i.key_pressed(egui::Key::F5) {
                self.show_disasm = !self.show_disasm;
            }
            // F6: Diff mode (if file B is loaded, toggle; otherwise open file dialog)
            if i.key_pressed(egui::Key::F6) {
                if self.state.diff.file_b.is_some() {
                    self.state.diff.active = !self.state.diff.active;
                } else if self.state.has_file() {
                    // Open file B dialog
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        if let Ok(mapped) = tv_core::MappedFile::open(&path) {
                            self.state.diff.file_b = Some(tv_ui::state::LoadedFile { path, mapped });
                            self.state.diff.viewport_b = tv_core::ViewPort::new(0, 4096);
                            self.state.diff.sync_scroll = true;
                            self.state.diff.active = true;
                            self.state.diff.clear();
                        }
                    }
                }
            }
            // F7: Structure Inspector
            if i.key_pressed(egui::Key::F7) {
                self.show_inspector = !self.show_inspector;
            }

            // F8: Byte Histogram
            if i.key_pressed(egui::Key::F8) {
                self.show_histogram = !self.show_histogram;
            }

            // F9: Cross-References
            if i.key_pressed(egui::Key::F9) {
                self.show_xrefs = !self.show_xrefs;
                // Build XRefs from current disassembly if available
                if self.show_xrefs && self.xrefs.table.is_none() {
                    if let Some(ref result) = self.disasm.result {
                        self.xrefs.build_from_instructions(&result.instructions);
                    }
                }
            }

            // F10: Bookmarks & Labels
            if i.key_pressed(egui::Key::F10) {
                self.show_bookmarks = !self.show_bookmarks;
            }
            // F11: Script Console
            if i.key_pressed(egui::Key::F11) {
                self.show_script = !self.show_script;
            }
            // Ctrl+F: Search
            if i.modifiers.ctrl && i.key_pressed(egui::Key::F) {
                self.show_search = !self.show_search;
            }
            // Ctrl+S: Save Session
            if i.modifiers.ctrl && !i.modifiers.shift && i.key_pressed(egui::Key::S) {
                if self.state.has_file() {
                    self.save_session();
                }
            }
            // Ctrl+Shift+S: Save Session As
            if i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::S) {
                if self.state.has_file() {
                    self.save_session_as();
                }
            }
            // Ctrl+O: Open file (standard shortcut)
            if i.modifiers.ctrl && i.key_pressed(egui::Key::O) {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    // Check if it's a session file
                    if path.extension().and_then(|e| e.to_str()) == Some(SESSION_EXTENSION) {
                        self.load_session(path);
                    } else {
                        self.open_file(path);
                    }
                }
            }
            // Workspace shortcuts: Ctrl+1 through Ctrl+5
            for (key, num) in [
                (egui::Key::Num1, 1u8),
                (egui::Key::Num2, 2u8),
                (egui::Key::Num3, 3u8),
                (egui::Key::Num4, 4u8),
                (egui::Key::Num5, 5u8),
            ] {
                if i.modifiers.ctrl && i.key_pressed(key) {
                    if let Some(idx) = self.workspaces.find_by_shortcut(num) {
                        self.apply_workspace(idx);
                    }
                }
            }
            // Escape: Close all floating windows and diff mode (except minimap)
            if i.key_pressed(egui::Key::Escape) {
                self.show_file_info = false;
                self.show_search = false;
                self.show_signatures = false;
                self.show_hilbert = false;
                self.show_disasm = false;
                self.show_inspector = false;
                self.show_script = false;
                self.state.diff.active = false;
                self.perf.visible = false;
            }
        });

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                // File menu
                ui.menu_button("File", |ui| {
                    if ui.button("Open...  (Ctrl+O)").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_file() {
                            self.open_file(path);
                        }
                        ui.close_menu();
                    }

                    ui.separator();

                    // Session management
                    if ui.button("Open Session...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("TitanView Session", &[SESSION_EXTENSION])
                            .pick_file()
                        {
                            self.load_session(path);
                        }
                        ui.close_menu();
                    }

                    let save_label = if self.session_modified {
                        "Save Session *  (Ctrl+S)"
                    } else {
                        "Save Session    (Ctrl+S)"
                    };
                    if ui.add_enabled(self.state.has_file(), egui::Button::new(save_label)).clicked() {
                        self.save_session();
                        ui.close_menu();
                    }

                    if ui.add_enabled(self.state.has_file(), egui::Button::new("Save Session As...  (Ctrl+Shift+S)")).clicked() {
                        self.save_session_as();
                        ui.close_menu();
                    }

                    if self.state.has_file() {
                        ui.separator();

                        if ui.button("Close Session").clicked() {
                            self.reset_to_landing();
                            ui.close_menu();
                        }

                        ui.menu_button("Export", |ui| {
                            if ui.button("Analysis report (JSON)").clicked() {
                                let json = tv_ui::export::export_json(&self.state);
                                if let Some(path) = rfd::FileDialog::new()
                                    .set_file_name("report.json")
                                    .add_filter("JSON", &["json"])
                                    .save_file()
                                {
                                    if let Err(e) = std::fs::write(&path, &json) {
                                        log::error!("Export failed: {}", e);
                                    } else {
                                        log::info!("Exported JSON to {}", path.display());
                                    }
                                }
                                ui.close_menu();
                            }
                            if self.state.search.results.is_some() {
                                if ui.button("Search results (CSV)").clicked() {
                                    let csv = tv_ui::export::export_search_csv(&self.state);
                                    if let Some(path) = rfd::FileDialog::new()
                                        .set_file_name("search_results.csv")
                                        .add_filter("CSV", &["csv"])
                                        .save_file()
                                    {
                                        if let Err(e) = std::fs::write(&path, &csv) {
                                            log::error!("Export failed: {}", e);
                                        }
                                    }
                                    ui.close_menu();
                                }
                            }
                            if self.state.signatures.is_some() || self.state.deep_scan.results.is_some() {
                                if ui.button("Signatures (CSV)").clicked() {
                                    let csv = tv_ui::export::export_signatures_csv(&self.state);
                                    if let Some(path) = rfd::FileDialog::new()
                                        .set_file_name("signatures.csv")
                                        .add_filter("CSV", &["csv"])
                                        .save_file()
                                    {
                                        if let Err(e) = std::fs::write(&path, &csv) {
                                            log::error!("Export failed: {}", e);
                                        }
                                    }
                                    ui.close_menu();
                                }
                            }
                        });
                    }

                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                // View menu
                ui.menu_button("View", |ui| {
                    if ui.checkbox(&mut self.show_file_info, "File Info  (F1)").clicked() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_minimap, "Minimap").clicked() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_hilbert, "Hilbert Curve  (F4)").clicked() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_disasm, "Disassembly  (F5)").clicked() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_inspector, "Struct Inspector  (F7)").clicked() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_histogram, "Byte Histogram    (F8)").clicked() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_xrefs, "Cross-References  (F9)").clicked() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_bookmarks, "Bookmarks/Labels  (F10)").clicked() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_script, "Script Console    (F11)").clicked() {
                        ui.close_menu();
                    }
                    // Diff controls
                    if self.state.diff.file_b.is_some() {
                        if ui.checkbox(&mut self.state.diff.active, "Binary Diff  (F6)").clicked() {
                            ui.close_menu();
                        }
                    } else if self.state.has_file() {
                        if ui.button("Open File B for Diff  (F6)").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_file() {
                                if let Ok(mapped) = tv_core::MappedFile::open(&path) {
                                    self.state.diff.file_b = Some(tv_ui::state::LoadedFile { path, mapped });
                                    self.state.diff.viewport_b = tv_core::ViewPort::new(0, 4096);
                                    self.state.diff.sync_scroll = true;
                                    self.state.diff.active = true;
                                    self.state.diff.clear();
                                }
                            }
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    if ui.checkbox(&mut self.perf.visible, "Performance  (F3)").clicked() {
                        ui.close_menu();
                    }
                });

                // Analysis menu
                ui.menu_button("Analysis", |ui| {
                    if ui.button("Search  (Ctrl+F)").clicked() {
                        self.show_search = true;
                        ui.close_menu();
                    }
                    if ui.button("Signatures  (F2)").clicked() {
                        self.show_signatures = true;
                        ui.close_menu();
                    }
                });

                // Workspace menu
                let current_ws = self.workspaces.active().clone();
                let mut workspace_to_apply: Option<usize> = None;

                ui.menu_button(format!("{} {}", current_ws.icon, current_ws.name), |ui| {
                    ui.label(egui::RichText::new("Workspaces").strong());
                    ui.label(egui::RichText::new("Switch analysis context").weak().small());
                    ui.separator();

                    // Collect workspace info to avoid borrow issues
                    let workspace_info: Vec<_> = self.workspaces.workspaces.iter().enumerate()
                        .map(|(idx, ws)| {
                            (idx, ws.icon.clone(), ws.name.clone(), ws.shortcut, ws.description.clone(),
                             idx == self.workspaces.active_index)
                        })
                        .collect();

                    for (idx, icon, name, shortcut, description, is_active) in workspace_info {
                        let shortcut_text = shortcut
                            .map(|n| format!("  (Ctrl+{})", n))
                            .unwrap_or_default();

                        let label = format!("{} {}{}", icon, name, shortcut_text);
                        let btn = egui::Button::new(label)
                            .selected(is_active);

                        let response = ui.add(btn)
                            .on_hover_text(&description);

                        if response.clicked() {
                            workspace_to_apply = Some(idx);
                            ui.close_menu();
                        }
                    }

                    ui.separator();
                    ui.weak("Workspaces configure window layouts,\nvisualization modes, and analysis tools.");
                });

                // Apply workspace after menu closes (avoids borrow conflict)
                if let Some(idx) = workspace_to_apply {
                    self.apply_workspace(idx);
                }

                // Show computation status
                if self.computing_entropy || self.computing_classification {
                    ui.separator();
                    ui.spinner();
                    ui.weak("Analyzing...");
                }

                // Right side: file name + FPS
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // FPS
                    ui.weak(format!("{:.0} FPS", self.perf.current_fps()));
                    ui.separator();

                    // File name
                    if self.state.has_file() {
                        ui.label(self.state.file_name());
                    }
                });
            });
        });

        // --- Floating Windows ---
        FileInfoWindow::show(ctx, &mut self.state, &mut self.show_file_info);
        SearchWindow::show(ctx, &mut self.state, &mut self.show_search);
        SignaturesWindow::show(ctx, &mut self.state, &mut self.show_signatures);
        HilbertWindow::show(ctx, &mut self.state, &mut self.hilbert, &mut self.show_hilbert);
        DisasmWindow::show(ctx, &mut self.state, &mut self.disasm, &mut self.show_disasm);
        StructInspector::show(ctx, &mut self.state, &mut self.inspector, &mut self.show_inspector);
        HistogramWindow::show(ctx, &mut self.state, &mut self.histogram, &mut self.show_histogram);
        XRefsWindow::show(ctx, &mut self.state, &mut self.xrefs, &mut self.show_xrefs);
        BookmarksWindow::show(ctx, &mut self.state, &mut self.bookmarks, &mut self.show_bookmarks);
        ScriptWindow::show(ctx, &mut self.state, &mut self.script, &mut self.show_script);
        PerfWindow::show(ctx, &mut self.perf);

        // Update inspector highlights in state
        self.state.inspector_highlights = self.inspector.highlight_offsets();

        // Right panel: minimap (only when file loaded and enabled)
        if self.state.has_file() && self.show_minimap {
            egui::SidePanel::right("minimap_panel")
                .default_width(60.0)
                .width_range(40.0..=100.0)
                .show(ctx, |ui| {
                    MinimapPanel::show(ui, &mut self.state, self.computing_entropy);
                });
        }

        // Bottom status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Session status
                if let Some(ref path) = self.session_path {
                    let indicator = if self.session_modified { "*" } else { "" };
                    ui.label(format!("Session: {}{}", path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown"), indicator));
                } else if self.state.has_file() {
                    ui.weak("No session");
                }

                ui.separator();

                // File info
                if self.state.has_file() {
                    ui.label(format!("{} | {} bytes | Offset: 0x{:X}",
                        self.state.file_name(),
                        self.state.file_len(),
                        self.state.viewport.start
                    ));
                }

                // Status message
                if let Some((msg, is_error)) = &self.session_status {
                    ui.separator();
                    let color = if *is_error {
                        egui::Color32::from_rgb(255, 100, 100)
                    } else {
                        egui::Color32::from_rgb(100, 200, 100)
                    };
                    ui.label(egui::RichText::new(msg).color(color));
                }

                // Spacer
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Computation progress
                    if self.computing_entropy || self.computing_classification {
                        ui.spinner();
                        ui.weak("Analyzing...");
                    }
                });
            });
        });

        // Central panel: hex view
        egui::CentralPanel::default().show(ctx, |ui| {
            if !self.state.has_file() {
                // Welcome screen
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() / 4.0);

                    ui.heading("TitanView");
                    ui.add_space(4.0);
                    ui.label("GPU-accelerated forensic data explorer");

                    ui.add_space(32.0);

                    if ui.button("Open File...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_file() {
                            self.pending_drop = Some(path);
                        }
                    }

                    ui.add_space(16.0);
                    ui.weak("or drag & drop a file");

                    ui.add_space(32.0);

                    // Keyboard shortcuts help
                    ui.group(|ui| {
                        ui.label("Keyboard Shortcuts");
                        ui.separator();
                        egui::Grid::new("shortcuts_grid")
                            .num_columns(2)
                            .spacing([20.0, 4.0])
                            .show(ui, |ui| {
                                ui.code("F1");
                                ui.label("File Info");
                                ui.end_row();

                                ui.code("F2");
                                ui.label("Signatures");
                                ui.end_row();

                                ui.code("F3");
                                ui.label("Performance");
                                ui.end_row();

                                ui.code("Ctrl+F");
                                ui.label("Search");
                                ui.end_row();

                                ui.code("Ctrl+G");
                                ui.label("Go to offset");
                                ui.end_row();

                                ui.code("Escape");
                                ui.label("Close windows");
                                ui.end_row();
                            });
                    });
                });
            } else if self.state.diff.active && self.state.diff.file_b.is_some() {
                // Diff mode: split view with two hex panels
                Self::show_diff_split_view(ui, &mut self.state);
            } else {
                HexPanel::show(ui, &mut self.state);
            }
        });
    }
}
