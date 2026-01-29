use egui::{Context, Color32, RichText, ScrollArea, text::LayoutJob, TextFormat, FontId};
use crate::state::{AppState, LoadedFile};
use tv_core::{FileRegion, MappedFile, ViewPort};

/// Floating window for binary diff comparison.
pub struct DiffWindow;

const BYTES_PER_ROW: usize = 16;
const ROW_HEIGHT: f32 = 16.0;

impl DiffWindow {
    pub fn show(ctx: &Context, state: &mut AppState, visible: &mut bool) {
        if !*visible {
            return;
        }

        egui::Window::new("Binary Diff")
            .open(visible)
            .default_size([900.0, 600.0])
            .min_size([600.0, 400.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                Self::show_contents(ui, state);
            });
    }

    fn show_contents(ui: &mut egui::Ui, state: &mut AppState) {
        // Top toolbar
        ui.horizontal(|ui| {
            // File A info
            if state.has_file() {
                ui.label(RichText::new("A:").strong());
                ui.label(state.file_name());
                ui.label(format!("({})", format_size(state.file_len())));
            } else {
                ui.label("A: No file");
            }

            ui.separator();

            // File B controls
            ui.label(RichText::new("B:").strong());
            if state.diff.file_b.is_some() {
                ui.label(state.diff.file_b_name());
                ui.label(format!("({})", format_size(state.diff.file_b_len())));
                if ui.small_button("Close").clicked() {
                    state.diff.close_file_b();
                }
            } else {
                if ui.button("Open file B...").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        if let Ok(mapped) = MappedFile::open(&path) {
                            state.diff.file_b = Some(LoadedFile { path, mapped });
                            state.diff.viewport_b = ViewPort::new(0, 4096);
                            state.diff.sync_scroll = true;
                            state.diff.clear();
                        }
                    }
                }
            }

            ui.separator();

            // Sync scroll toggle
            ui.checkbox(&mut state.diff.sync_scroll, "Sync scroll");
        });

        ui.separator();

        // Check if both files are loaded
        if !state.has_file() {
            ui.centered_and_justified(|ui| {
                ui.label("Open file A first (File > Open)");
            });
            return;
        }

        if state.diff.file_b.is_none() {
            ui.centered_and_justified(|ui| {
                ui.label("Open file B to compare");
            });
            return;
        }

        // Diff controls
        ui.horizontal(|ui| {
            if state.diff.computing {
                ui.spinner();
                ui.label("Computing diff...");
            } else {
                if ui.button("Compute Diff").clicked() {
                    state.diff.computing = true;
                }

                if let Some(count) = state.diff.diff_offsets.as_ref().map(|v| v.len()) {
                    let total = state.diff.diff_count;
                    if total > count as u64 {
                        ui.label(format!("{} differences (showing first {})", total, count));
                    } else {
                        ui.label(format!("{} differences", count));
                    }

                    if let Some(ms) = state.diff.compute_time_ms {
                        ui.weak(format!("({:.1} ms)", ms));
                    }

                    // Navigation
                    ui.separator();
                    let sel = state.diff.selected_diff.unwrap_or(0);
                    if ui.button("<").on_hover_text("Previous diff").clicked() {
                        if sel > 0 {
                            state.diff.selected_diff = Some(sel - 1);
                            if let Some(offsets) = &state.diff.diff_offsets {
                                if let Some(&offset) = offsets.get(sel - 1) {
                                    state.viewport.start = (offset / 16) * 16;
                                    if state.diff.sync_scroll {
                                        state.diff.viewport_b.start = state.viewport.start;
                                    }
                                }
                            }
                        }
                    }
                    ui.label(format!("{}/{}", sel + 1, count));
                    if ui.button(">").on_hover_text("Next diff").clicked() {
                        if sel + 1 < count {
                            state.diff.selected_diff = Some(sel + 1);
                            if let Some(offsets) = &state.diff.diff_offsets {
                                if let Some(&offset) = offsets.get(sel + 1) {
                                    state.viewport.start = (offset / 16) * 16;
                                    if state.diff.sync_scroll {
                                        state.diff.viewport_b.start = state.viewport.start;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        ui.separator();

        // Split view with two hex panels
        let available = ui.available_size();
        let panel_width = ((available.x - 20.0) / 2.0).max(300.0);

        ui.columns(2, |columns| {
            // Left panel (File A)
            columns[0].vertical(|ui| {
                ui.set_max_width(panel_width);
                ui.label(RichText::new("File A").strong().color(Color32::from_rgb(100, 200, 100)));
                Self::show_hex_panel(ui, state, true);
            });

            // Right panel (File B)
            columns[1].vertical(|ui| {
                ui.set_max_width(panel_width);
                ui.label(RichText::new("File B").strong().color(Color32::from_rgb(200, 100, 100)));
                Self::show_hex_panel(ui, state, false);
            });
        });

        // Sync scroll handling
        if state.diff.sync_scroll {
            state.diff.viewport_b.start = state.viewport.start;
        }
    }

    fn show_hex_panel(ui: &mut egui::Ui, state: &mut AppState, is_file_a: bool) {
        // Get file info first to avoid borrow issues
        let (file_len, viewport_start) = if is_file_a {
            (state.file.as_ref().map(|f| f.mapped.len()).unwrap_or(0), state.viewport.start)
        } else {
            (state.diff.file_b.as_ref().map(|f| f.mapped.len()).unwrap_or(0), state.diff.viewport_b.start)
        };

        if file_len == 0 {
            return;
        }

        let available_height = ui.available_height() - 20.0;
        let visible_rows = (available_height / ROW_HEIGHT) as usize;
        let total_rows = (file_len as usize + BYTES_PER_ROW - 1) / BYTES_PER_ROW;

        // Rebuild diff highlights for current viewport
        let vp_end = viewport_start + (visible_rows * BYTES_PER_ROW) as u64;
        state.diff.rebuild_highlights_for_viewport(viewport_start, vp_end);

        // Clone highlight set to avoid borrow issues in the closure
        let highlight_set = state.diff.highlight_set.clone();

        // Get reference to the file for the scroll area
        let file = if is_file_a {
            state.file.as_ref()
        } else {
            state.diff.file_b.as_ref()
        };
        let file = match file {
            Some(f) => f,
            None => return,
        };

        let mono_font = FontId::monospace(12.0);
        let normal_color = Color32::from_rgb(200, 200, 200);
        let diff_color = Color32::from_rgb(255, 100, 100);
        let offset_color = Color32::from_rgb(100, 150, 200);
        let ascii_color = Color32::from_rgb(150, 150, 150);

        ScrollArea::vertical()
            .id_salt(if is_file_a { "diff_hex_a" } else { "diff_hex_b" })
            .auto_shrink([false, false])
            .show_rows(ui, ROW_HEIGHT, total_rows, |ui, row_range| {
                for row_idx in row_range {
                    let row_offset = (row_idx * BYTES_PER_ROW) as u64;
                    let row_end = (row_offset + BYTES_PER_ROW as u64).min(file_len);
                    let row_len = (row_end - row_offset) as usize;

                    if row_len == 0 {
                        continue;
                    }

                    let data = file.mapped.slice(FileRegion::new(row_offset, row_len as u64));

                    // Build a single LayoutJob for the entire row
                    let mut job = LayoutJob::default();

                    // Offset
                    job.append(
                        &format!("{:08X}  ", row_offset),
                        0.0,
                        TextFormat {
                            font_id: mono_font.clone(),
                            color: offset_color,
                            ..Default::default()
                        },
                    );

                    // Hex bytes
                    for (i, &byte) in data.iter().enumerate() {
                        let byte_offset = row_offset + i as u64;
                        let is_diff = highlight_set.contains(&byte_offset);
                        let color = if is_diff { diff_color } else { normal_color };

                        let suffix = if i == 7 { "  " } else { " " };
                        job.append(
                            &format!("{:02X}{}", byte, suffix),
                            0.0,
                            TextFormat {
                                font_id: mono_font.clone(),
                                color,
                                background: if is_diff {
                                    Color32::from_rgba_unmultiplied(255, 0, 0, 40)
                                } else {
                                    Color32::TRANSPARENT
                                },
                                ..Default::default()
                            },
                        );
                    }

                    // Padding for short rows
                    for i in row_len..BYTES_PER_ROW {
                        let suffix = if i == 7 { "  " } else { " " };
                        job.append(
                            &format!("  {}", suffix),
                            0.0,
                            TextFormat {
                                font_id: mono_font.clone(),
                                color: normal_color,
                                ..Default::default()
                            },
                        );
                    }

                    // Separator
                    job.append(
                        " ",
                        0.0,
                        TextFormat {
                            font_id: mono_font.clone(),
                            color: normal_color,
                            ..Default::default()
                        },
                    );

                    // ASCII
                    for (i, &byte) in data.iter().enumerate() {
                        let byte_offset = row_offset + i as u64;
                        let is_diff = highlight_set.contains(&byte_offset);
                        let c = if byte >= 0x20 && byte <= 0x7E { byte as char } else { '.' };
                        let color = if is_diff { diff_color } else { ascii_color };

                        job.append(
                            &c.to_string(),
                            0.0,
                            TextFormat {
                                font_id: mono_font.clone(),
                                color,
                                background: if is_diff {
                                    Color32::from_rgba_unmultiplied(255, 0, 0, 40)
                                } else {
                                    Color32::TRANSPARENT
                                },
                                ..Default::default()
                            },
                        );
                    }

                    ui.label(job);
                }
            });

        // Handle scroll for non-synced mode
        if !state.diff.sync_scroll && !is_file_a {
            // TODO: Capture scroll events for independent scrolling
        }
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
