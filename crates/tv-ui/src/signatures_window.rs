use egui::{Context, Color32, RichText};
use crate::state::{AppState, SignaturesTab, SignatureHit, SignatureSortOrder, SignatureCategory};
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

        // Get count without cloning
        let sig_count = state.signatures.as_ref().map(|s| s.len());

        if let Some(count) = sig_count {
            ui.label(format!("{} signature(s) detected", count));
            ui.add_space(4.0);

            if count == 0 {
                ui.label("No signatures found in first 1 MB.");
            } else {
                let mut clicked_offset: Option<u64> = None;
                let mut export_index: Option<usize> = None;

                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .id_salt("quick_sig_scroll")
                    .show_rows(ui, ROW_HEIGHT + 2.0, count, |ui, row_range| {
                        let sigs = match &state.signatures {
                            Some(s) => s,
                            None => return,
                        };

                        for i in row_range {
                            if let Some(sig) = sigs.get(i) {
                                ui.horizontal(|ui| {
                                    let text = format!("{} @ 0x{:X}", sig.name, sig.offset);
                                    let color = signature_color(&sig.name);
                                    if ui.selectable_label(false, RichText::new(&text).color(color)).clicked() {
                                        clicked_offset = Some(sig.offset);
                                    }

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.small_button("Export").clicked() {
                                            export_index = Some(i);
                                        }
                                    });
                                });
                            }
                        }
                    });

                // Apply deferred actions
                if let Some(offset) = clicked_offset {
                    state.viewport.start = (offset / 16) * 16;
                }

                if let Some(i) = export_index {
                    if let Some(sigs) = &state.signatures {
                        if let Some(sig) = sigs.get(i) {
                            let sig_clone = sig.clone();
                            if let Some(path) = Self::export_signature(state, &sig_clone, None) {
                                log::info!("Exported {} to {}", sig_clone.name, path.display());
                            }
                        }
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
        let scanning = state.deep_scan.scanning;
        let duration_ms = state.deep_scan.duration_ms;
        let total_count = state.deep_scan.total_count();
        let filtered_count = state.deep_scan.filtered_count();

        if total_count == 0 && !scanning {
            return;
        }

        if !scanning && total_count > 0 {
            ui.separator();

            // Stats
            ui.horizontal(|ui| {
                if filtered_count == total_count {
                    ui.strong(format!("{} signature(s)", total_count));
                } else {
                    ui.strong(format!("{} / {} signature(s)", filtered_count, total_count));
                }
                if let Some(ms) = duration_ms {
                    ui.weak(format!("in {:.1} ms", ms));
                }
            });

            // Filter and sort controls
            ui.add_space(4.0);
            let mut needs_rebuild = false;

            ui.horizontal(|ui| {
                ui.label("Filter:");

                // Category filter dropdown
                egui::ComboBox::from_id_salt("sig_category")
                    .selected_text(state.deep_scan.filter_category.label())
                    .width(90.0)
                    .show_ui(ui, |ui| {
                        for cat in [
                            SignatureCategory::All,
                            SignatureCategory::Executables,
                            SignatureCategory::Archives,
                            SignatureCategory::Images,
                            SignatureCategory::Documents,
                            SignatureCategory::Databases,
                            SignatureCategory::Other,
                        ] {
                            if ui.selectable_value(&mut state.deep_scan.filter_category, cat, cat.label()).changed() {
                                needs_rebuild = true;
                            }
                        }
                    });

                // Text filter
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.deep_scan.filter_text)
                        .hint_text("Search...")
                        .desired_width(80.0)
                );
                if response.changed() {
                    needs_rebuild = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Sort:");

                egui::ComboBox::from_id_salt("sig_sort")
                    .selected_text(match state.deep_scan.sort_order {
                        SignatureSortOrder::OffsetAsc => "Offset (asc)",
                        SignatureSortOrder::OffsetDesc => "Offset (desc)",
                        SignatureSortOrder::NameAsc => "Name (A-Z)",
                        SignatureSortOrder::NameDesc => "Name (Z-A)",
                        SignatureSortOrder::TypeAsc => "Type",
                    })
                    .width(100.0)
                    .show_ui(ui, |ui| {
                        for (order, label) in [
                            (SignatureSortOrder::OffsetAsc, "Offset (asc)"),
                            (SignatureSortOrder::OffsetDesc, "Offset (desc)"),
                            (SignatureSortOrder::NameAsc, "Name (A-Z)"),
                            (SignatureSortOrder::NameDesc, "Name (Z-A)"),
                            (SignatureSortOrder::TypeAsc, "Type"),
                        ] {
                            if ui.selectable_value(&mut state.deep_scan.sort_order, order, label).changed() {
                                needs_rebuild = true;
                            }
                        }
                    });
            });

            if needs_rebuild {
                state.deep_scan.rebuild_filtered_indices();
            }

            // Export buttons
            let mut export_selected = false;
            let mut export_all = false;

            if filtered_count > 0 {
                ui.horizontal(|ui| {
                    if ui.button("Export Selected").clicked() {
                        export_selected = true;
                    }
                    if ui.button("Export All...").on_hover_text("Export filtered signatures").clicked() {
                        export_all = true;
                    }
                });
            }

            // Handle exports (deferred)
            if export_selected {
                if let Some(sig) = state.deep_scan.selected_result
                    .and_then(|sel| state.deep_scan.get_filtered_signature(sel))
                {
                    let sig_clone = sig.clone();
                    if let Some(path) = Self::export_signature(state, &sig_clone, None) {
                        log::info!("Exported {} to {}", sig_clone.name, path.display());
                    }
                }
            }

            if export_all {
                Self::export_filtered_signatures(state);
            }
        }

        // Results list with navigation
        if filtered_count > 0 {
            let sel = state.deep_scan.selected_result.unwrap_or(0);

            // Navigation
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("|<").on_hover_text("First").clicked() {
                    state.deep_scan.selected_result = Some(0);
                    state.deep_scan.update_highlight();
                    if let Some(sig) = state.deep_scan.get_filtered_signature(0) {
                        state.viewport.start = (sig.offset / 16) * 16;
                    }
                }
                if ui.button("<").on_hover_text("Previous").clicked() && sel > 0 {
                    let new_sel = sel - 1;
                    state.deep_scan.selected_result = Some(new_sel);
                    state.deep_scan.update_highlight();
                    if let Some(sig) = state.deep_scan.get_filtered_signature(new_sel) {
                        state.viewport.start = (sig.offset / 16) * 16;
                    }
                }
                ui.label(format!("{} / {}", sel + 1, filtered_count));
                if ui.button(">").on_hover_text("Next").clicked() && sel + 1 < filtered_count {
                    let new_sel = sel + 1;
                    state.deep_scan.selected_result = Some(new_sel);
                    state.deep_scan.update_highlight();
                    if let Some(sig) = state.deep_scan.get_filtered_signature(new_sel) {
                        state.viewport.start = (sig.offset / 16) * 16;
                    }
                }
                if ui.button(">|").on_hover_text("Last").clicked() {
                    let last = filtered_count - 1;
                    state.deep_scan.selected_result = Some(last);
                    state.deep_scan.update_highlight();
                    if let Some(sig) = state.deep_scan.get_filtered_signature(last) {
                        state.viewport.start = (sig.offset / 16) * 16;
                    }
                }
            });

            ui.add_space(4.0);

            // Track actions to apply after UI iteration
            let mut clicked_index: Option<usize> = None;
            let mut export_index: Option<usize> = None;

            // Results list - only renders visible rows using filtered_indices
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .id_salt("deep_scan_scroll")
                .show_rows(ui, ROW_HEIGHT + 2.0, filtered_count, |ui, row_range| {
                    for i in row_range {
                        if let Some(sig) = state.deep_scan.get_filtered_signature(i) {
                            let selected = state.deep_scan.selected_result == Some(i);

                            ui.horizontal(|ui| {
                                let text = format!("{} @ 0x{:X}", sig.name, sig.offset);
                                let color = signature_color(&sig.name);
                                let label = egui::RichText::new(&text).color(color);

                                if ui.selectable_label(selected, label).clicked() {
                                    clicked_index = Some(i);
                                }

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button("Export").clicked() {
                                        export_index = Some(i);
                                    }
                                });
                            });
                        }
                    }
                });

            // Apply deferred actions
            if let Some(i) = clicked_index {
                if let Some(sig) = state.deep_scan.get_filtered_signature(i) {
                    let offset = sig.offset;
                    state.deep_scan.selected_result = Some(i);
                    state.deep_scan.update_highlight();
                    state.viewport.start = (offset / 16) * 16;
                }
            }

            if let Some(i) = export_index {
                if let Some(sig) = state.deep_scan.get_filtered_signature(i) {
                    let sig_clone = sig.clone();
                    if let Some(path) = Self::export_signature(state, &sig_clone, None) {
                        log::info!("Exported {} to {}", sig_clone.name, path.display());
                    }
                }
            }
        } else if !scanning && total_count > 0 {
            ui.label("No signatures match the current filter.");
        }

        // Clear button
        if !scanning && total_count > 0 {
            ui.add_space(4.0);
            if ui.button("Clear Results").clicked() {
                state.deep_scan.results = None;
                state.deep_scan.duration_ms = None;
                state.deep_scan.selected_result = None;
                state.deep_scan.filtered_indices.clear();
                state.deep_scan.clear_highlight();
            }
        }
    }

    /// Export filtered signatures to a folder.
    fn export_filtered_signatures(state: &AppState) {
        // Collect filtered signatures
        let filtered: Vec<SignatureHit> = (0..state.deep_scan.filtered_count())
            .filter_map(|i| state.deep_scan.get_filtered_signature(i).cloned())
            .collect();

        if filtered.is_empty() {
            return;
        }

        Self::export_all_signatures(state, &filtered);
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
