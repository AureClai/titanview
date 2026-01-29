use egui::{Context, Color32, RichText};
use crate::state::{AppState, SignaturesTab, SignatureHit};
use tv_core::{analyze_carve_size, FileRegion};
use std::path::PathBuf;

/// Floating window for signature detection (quick scan + deep scan).
pub struct SignaturesWindow;

/// Row height for virtual scroll.
const ROW_HEIGHT: f32 = 18.0;

impl SignaturesWindow {
    pub fn show(ctx: &Context, state: &mut AppState, visible: &mut bool) {
        if !*visible {
            return;
        }

        egui::Window::new("Signatures")
            .open(visible)
            .default_size([350.0, 450.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                Self::show_contents(ui, state);
            });
    }

    fn show_contents(ui: &mut egui::Ui, state: &mut AppState) {
        if !state.has_file() {
            ui.label("Open a file first.");
            return;
        }

        // Tab-like selection
        ui.horizontal(|ui| {
            ui.selectable_value(&mut state.signatures_tab, SignaturesTab::QuickScan, "Quick Scan");
            ui.selectable_value(&mut state.signatures_tab, SignaturesTab::DeepScan, "Deep Scan");
        });

        ui.separator();

        match state.signatures_tab {
            SignaturesTab::QuickScan => Self::show_quick_scan(ui, state),
            SignaturesTab::DeepScan => Self::show_deep_scan(ui, state),
        }
    }

    /// Export a carved file to disk.
    fn export_signature(state: &AppState, sig: &SignatureHit, custom_size: Option<u64>) -> Option<PathBuf> {
        let file = state.file.as_ref()?;
        let file_len = file.mapped.len();

        // Get data starting at signature offset (max 64MB for analysis)
        let max_analyze = 64 * 1024 * 1024u64;
        let analyze_len = max_analyze.min(file_len - sig.offset);
        let data = file.mapped.slice(FileRegion::new(sig.offset, analyze_len));

        // Analyze carve size
        let carve = analyze_carve_size(&sig.name, data, analyze_len);

        // Determine final size
        let export_size = custom_size
            .or(carve.size)
            .unwrap_or(4096)  // Default 4KB if unknown
            .min(file_len - sig.offset);

        // Suggest filename
        let extension = carve.extension;
        let suggested_name = format!("carved_0x{:X}.{}", sig.offset, extension);

        // Show save dialog
        let save_path = rfd::FileDialog::new()
            .set_file_name(&suggested_name)
            .add_filter("Carved file", &[extension])
            .add_filter("All files", &["*"])
            .save_file()?;

        // Extract and save
        let export_data = file.mapped.slice(FileRegion::new(sig.offset, export_size));
        if std::fs::write(&save_path, export_data).is_ok() {
            Some(save_path)
        } else {
            None
        }
    }

    fn show_quick_scan(ui: &mut egui::Ui, state: &mut AppState) {
        ui.label(RichText::new("Scans first 1 MB at fixed offsets (instant)").weak().small());
        ui.add_space(4.0);

        // Clone signatures to avoid borrow issues
        let sigs_clone = state.signatures.clone();

        if let Some(ref sigs) = sigs_clone {
            ui.label(format!("{} signature(s) detected", sigs.len()));
            ui.add_space(4.0);

            if sigs.is_empty() {
                ui.label("No signatures found in first 1 MB.");
            } else {
                let mut export_request: Option<SignatureHit> = None;

                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .id_salt("quick_sig_scroll")
                    .show(ui, |ui| {
                        for sig in sigs {
                            ui.horizontal(|ui| {
                                // Signature name and offset
                                let text = format!("{} @ 0x{:X}", sig.name, sig.offset);
                                let color = signature_color(&sig.name);
                                if ui.selectable_label(false, RichText::new(&text).color(color)).clicked() {
                                    state.viewport.start = (sig.offset / 16) * 16;
                                }

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button("Export").clicked() {
                                        export_request = Some(sig.clone());
                                    }
                                });
                            });
                        }
                    });

                // Handle export request after UI iteration
                if let Some(sig) = export_request {
                    if let Some(path) = Self::export_signature(state, &sig, None) {
                        log::info!("Exported {} to {}", sig.name, path.display());
                    }
                }
            }
        } else {
            ui.label("Quick scan runs automatically when file is opened.");
        }
    }

    fn show_deep_scan(ui: &mut egui::Ui, state: &mut AppState) {
        ui.label(RichText::new("GPU multi-pattern scan (entire file)").weak().small());
        ui.add_space(4.0);

        // Scan button or progress
        if state.deep_scan.scanning {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Scanning...");
            });

            // Progress bar
            if state.deep_scan.total_bytes > 0 {
                let progress = state.deep_scan.bytes_scanned as f32 / state.deep_scan.total_bytes as f32;
                ui.add(egui::ProgressBar::new(progress)
                    .text(format!(
                        "{:.1}% ({} / {})",
                        progress * 100.0,
                        format_size_short(state.deep_scan.bytes_scanned),
                        format_size_short(state.deep_scan.total_bytes)
                    ))
                    .animate(true));

                // Intermediate results
                if let Some(ref results) = state.deep_scan.results {
                    if !results.is_empty() {
                        ui.weak(format!("{} found so far...", results.len()));
                    }
                }
            }
        } else if state.deep_scan.results.is_none() {
            if ui.button("Start Deep Scan").clicked() {
                state.deep_scan.scanning = true;
            }
            ui.label(RichText::new("Scans entire file for 37 signatures").weak().small());
        }

        // Results
        Self::show_deep_scan_results(ui, state);
    }

    fn show_deep_scan_results(ui: &mut egui::Ui, state: &mut AppState) {
        // Clone results to avoid borrow issues
        let results_clone = state.deep_scan.results.clone();
        let scanning = state.deep_scan.scanning;
        let duration_ms = state.deep_scan.duration_ms;

        let results_info = results_clone.as_ref().map(|results| {
            let count = results.len();
            let first_offset = results.first().map(|s| s.offset);
            let last_offset = results.last().map(|s| s.offset);
            let sel = state.deep_scan.selected_result.unwrap_or(0);
            let prev_offset = if sel > 0 { results.get(sel - 1).map(|s| s.offset) } else { None };
            let next_offset = results.get(sel + 1).map(|s| s.offset);
            (count, first_offset, last_offset, prev_offset, next_offset, sel, results.clone())
        });

        if let Some((count, first_offset, last_offset, prev_offset, next_offset, sel, results)) = results_info {
            if !scanning {
                ui.separator();

                // Stats
                ui.horizontal(|ui| {
                    ui.strong(format!("{} signature(s)", count));
                    if let Some(ms) = duration_ms {
                        ui.weak(format!("in {:.1} ms", ms));
                    }
                });

                // Export selected button
                if count > 0 {
                    ui.horizontal(|ui| {
                        if ui.button("Export Selected").clicked() {
                            if let Some(sig) = results.get(sel) {
                                if let Some(path) = Self::export_signature(state, sig, None) {
                                    log::info!("Exported {} to {}", sig.name, path.display());
                                }
                            }
                        }

                        if ui.button("Export All...").on_hover_text("Export all signatures to a folder").clicked() {
                            Self::export_all_signatures(state, &results);
                        }
                    });
                }
            }

            if count == 0 && !scanning {
                ui.label("No embedded signatures detected.");
            } else if count > 0 {
                // Navigation
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("|<").on_hover_text("First").clicked() {
                        if let Some(offset) = first_offset {
                            state.deep_scan.selected_result = Some(0);
                            state.deep_scan.update_highlight();
                            state.viewport.start = (offset / 16) * 16;
                        }
                    }
                    if ui.button("<").on_hover_text("Previous").clicked() {
                        if let Some(offset) = prev_offset {
                            state.deep_scan.selected_result = Some(sel - 1);
                            state.deep_scan.update_highlight();
                            state.viewport.start = (offset / 16) * 16;
                        }
                    }
                    ui.label(format!("{} / {}", sel + 1, count));
                    if ui.button(">").on_hover_text("Next").clicked() {
                        if let Some(offset) = next_offset {
                            state.deep_scan.selected_result = Some(sel + 1);
                            state.deep_scan.update_highlight();
                            state.viewport.start = (offset / 16) * 16;
                        }
                    }
                    if ui.button(">|").on_hover_text("Last").clicked() {
                        if let Some(offset) = last_offset {
                            state.deep_scan.selected_result = Some(count - 1);
                            state.deep_scan.update_highlight();
                            state.viewport.start = (offset / 16) * 16;
                        }
                    }
                });

                ui.add_space(4.0);

                // Track export request
                let mut export_request: Option<SignatureHit> = None;

                // Results list with export buttons
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .id_salt("deep_scan_scroll")
                    .show_rows(ui, ROW_HEIGHT + 2.0, count, |ui, row_range| {
                        for i in row_range {
                            if let Some(sig) = results.get(i) {
                                let selected = state.deep_scan.selected_result == Some(i);

                                ui.horizontal(|ui| {
                                    let text = format!("{} @ 0x{:X}", sig.name, sig.offset);
                                    let color = signature_color(&sig.name);
                                    let label = egui::RichText::new(&text).color(color);

                                    if ui.selectable_label(selected, label).clicked() {
                                        state.deep_scan.selected_result = Some(i);
                                        state.deep_scan.update_highlight();
                                        state.viewport.start = (sig.offset / 16) * 16;
                                    }

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.small_button("Export").clicked() {
                                            export_request = Some(sig.clone());
                                        }
                                    });
                                });
                            }
                        }
                    });

                // Handle export after UI iteration
                if let Some(sig) = export_request {
                    if let Some(path) = Self::export_signature(state, &sig, None) {
                        log::info!("Exported {} to {}", sig.name, path.display());
                    }
                }
            }

            // Clear button
            if !scanning {
                ui.add_space(4.0);
                if ui.button("Clear Results").clicked() {
                    state.deep_scan.results = None;
                    state.deep_scan.duration_ms = None;
                    state.deep_scan.selected_result = None;
                    state.deep_scan.clear_highlight();
                }
            }
        }
    }

    /// Export all signatures to a folder.
    fn export_all_signatures(state: &AppState, results: &[SignatureHit]) {
        // Pick a folder
        let folder: PathBuf = match rfd::FileDialog::new().pick_folder() {
            Some(f) => f,
            None => return,
        };

        let file = match &state.file {
            Some(f) => f,
            None => return,
        };
        let file_len = file.mapped.len();

        let mut exported = 0;
        let max_analyze = 64 * 1024 * 1024u64;

        for (i, sig) in results.iter().enumerate() {
            let analyze_len = max_analyze.min(file_len - sig.offset);
            let data = file.mapped.slice(FileRegion::new(sig.offset, analyze_len));
            let carve = analyze_carve_size(&sig.name, data, analyze_len);

            let export_size = carve.size
                .unwrap_or(4096)
                .min(file_len - sig.offset);

            let extension = carve.extension;
            let filename = format!("{:04}_0x{:X}.{}", i, sig.offset, extension);
            let path = folder.join(filename);

            let export_data = file.mapped.slice(FileRegion::new(sig.offset, export_size));
            if std::fs::write(&path, export_data).is_ok() {
                exported += 1;
            }
        }

        log::info!("Exported {} / {} signatures to {}", exported, results.len(), folder.display());
    }
}

/// Format bytes into a short human-readable string.
fn format_size_short(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Color-code signatures by type.
fn signature_color(name: &str) -> Color32 {
    let name_lower = name.to_lowercase();

    if name_lower.contains("exe") || name_lower.contains("elf") || name_lower.contains("mach") {
        Color32::from_rgb(255, 150, 150) // Executables - red
    } else if name_lower.contains("zip") || name_lower.contains("gz") || name_lower.contains("7z")
        || name_lower.contains("rar") || name_lower.contains("tar") {
        Color32::from_rgb(150, 200, 255) // Archives - blue
    } else if name_lower.contains("png") || name_lower.contains("jpg") || name_lower.contains("gif")
        || name_lower.contains("bmp") || name_lower.contains("webp") {
        Color32::from_rgb(150, 255, 150) // Images - green
    } else if name_lower.contains("pdf") || name_lower.contains("doc") || name_lower.contains("xml") {
        Color32::from_rgb(255, 200, 100) // Documents - orange
    } else if name_lower.contains("sqlite") || name_lower.contains("db") {
        Color32::from_rgb(200, 150, 255) // Databases - purple
    } else {
        Color32::from_rgb(200, 200, 200) // Other - gray
    }
}
