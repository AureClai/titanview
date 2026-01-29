use egui::{Ui, ScrollArea, Color32, RichText, FontId, Sense};
use tv_core::FileRegion;
use crate::state::AppState;
use crate::minimap_panel::class_to_subtle_bg;

/// Color for modified bytes in edit mode.
const EDIT_COLOR: Color32 = Color32::from_rgb(255, 100, 100);
const EDIT_BG: Color32 = Color32::from_rgb(80, 0, 0);
/// Color for selected byte in edit mode.
const SELECTED_COLOR: Color32 = Color32::from_rgb(255, 255, 100);
const SELECTED_BG: Color32 = Color32::from_rgb(80, 80, 0);

/// Bytes per row in the hex view.
const BYTES_PER_ROW: u64 = 16;
/// Height of one monospace row in pixels.
const ROW_HEIGHT: f32 = 18.0;
/// Maximum rows that egui f32 scroll can handle reliably (~8M rows = 128 MB).
const MAX_DIRECT_ROWS: u64 = 8_000_000;

/// Hex view panel with virtual scrolling.
pub struct HexPanel;

impl HexPanel {
    pub fn show(ui: &mut Ui, state: &mut AppState) {
        if !state.has_file() {
            ui.label("Open a file to view its contents.");
            return;
        }

        let file_len = state.file_len();
        let total_rows = file_len.div_ceil(BYTES_PER_ROW);

        // Show edit mode toolbar and dialogs
        Self::show_edit_toolbar(ui, state);
        Self::show_edit_dialogs(ui, state);

        // Keyboard navigation
        Self::handle_keyboard(ui, state, file_len);

        // For large files (>128 MB), show a coarse navigation slider
        let coarse_offset = if total_rows > MAX_DIRECT_ROWS {
            Self::show_coarse_slider(ui, file_len, state)
        } else {
            0u64
        };

        let window_rows = if total_rows > MAX_DIRECT_ROWS {
            // Show at most MAX_DIRECT_ROWS in the inner scroll
            let remaining_bytes = file_len.saturating_sub(coarse_offset);
            let remaining_rows = remaining_bytes.div_ceil(BYTES_PER_ROW);
            remaining_rows.min(MAX_DIRECT_ROWS) as usize
        } else {
            total_rows as usize
        };

        ui.separator();

        // Rebuild search highlights for the visible viewport (cheap: binary search + small set)
        {
            let vp_start = state.viewport.start;
            // Estimate visible bytes (generous: ~64 rows)
            let vp_end = vp_start.saturating_add(BYTES_PER_ROW * 64).min(file_len);
            state.search.rebuild_highlights_for_viewport(vp_start, vp_end);
        }

        // Get references to all highlight sets
        let search_highlights = &state.search.highlight_set;
        let deep_scan_highlights = &state.deep_scan.highlight_set;
        let inspector_highlights = &state.inspector_highlights;
        let has_highlights = !search_highlights.is_empty() || !deep_scan_highlights.is_empty() || !inspector_highlights.is_empty();

        // Capture edit state for the closure
        let edit_enabled = state.edit.enabled;
        let selected_offset = state.edit.selected_offset;
        let pending_edits = state.edit.pending_edits.clone();
        let mut clicked_offset: Option<u64> = None;

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, ROW_HEIGHT, window_rows, |ui, row_range| {
                let file = match &state.file {
                    Some(f) => f,
                    None => return,
                };

                ui.style_mut().override_font_id = Some(FontId::monospace(13.0));

                for row_idx in row_range {
                    let byte_offset = coarse_offset + (row_idx as u64) * BYTES_PER_ROW;
                    if byte_offset >= file_len {
                        break;
                    }

                    let region = FileRegion::new(byte_offset, BYTES_PER_ROW);
                    let data = file.mapped.slice(region);

                    // Compute classification background for this row's offset column
                    let class_bg = state.classification.as_ref().and_then(|c| {
                        let block_idx = (byte_offset / 256) as usize;
                        c.get(block_idx).map(|&v| class_to_subtle_bg(v))
                    });

                    // Helper to get effective byte value (with edits applied)
                    let get_byte = |abs: u64, original: u8| -> u8 {
                        pending_edits.get(&abs).copied().unwrap_or(original)
                    };

                    // Helper to check if byte is modified
                    let is_modified = |abs: u64| -> bool {
                        pending_edits.contains_key(&abs)
                    };

                    // Helper to check if byte is selected
                    let is_selected = |abs: u64| -> bool {
                        selected_offset == Some(abs)
                    };

                    if !has_highlights && !edit_enabled {
                        // No highlights and not in edit mode — use fast single-label path
                        let line = format_hex_line(byte_offset, data);
                        ui.horizontal(|ui| {
                            let offset_text = RichText::new(&line.offset)
                                .color(Color32::from_rgb(100, 140, 180))
                                .background_color(class_bg.unwrap_or(Color32::TRANSPARENT));
                            ui.label(offset_text);
                            ui.label(RichText::new(&line.hex).color(Color32::from_rgb(220, 220, 220)));
                            ui.label(RichText::new(&line.ascii).color(Color32::from_rgb(160, 200, 140)));
                        });
                    } else {
                        // Highlighted path or edit mode: build a rich-text layout per byte
                        let line = format_hex_line(byte_offset, data);
                        ui.horizontal(|ui| {
                            let offset_text = RichText::new(&line.offset)
                                .color(Color32::from_rgb(100, 140, 180))
                                .background_color(class_bg.unwrap_or(Color32::TRANSPARENT));
                            ui.label(offset_text);

                            // Helper to check if byte is highlighted
                            let is_highlighted = |abs: u64| -> bool {
                                search_highlights.contains(&abs) || deep_scan_highlights.contains(&abs) || inspector_highlights.contains(&abs)
                            };

                            // Different colors for different highlight types (with edit mode overrides)
                            let get_colors = |abs: u64| -> (Color32, Color32) {
                                // Edit mode colors take priority
                                if is_selected(abs) {
                                    return (SELECTED_COLOR, SELECTED_BG);
                                }
                                if is_modified(abs) {
                                    return (EDIT_COLOR, EDIT_BG);
                                }
                                // Then search/highlight colors
                                if search_highlights.contains(&abs) {
                                    (Color32::from_rgb(255, 255, 80), Color32::from_rgb(50, 50, 0))
                                } else if deep_scan_highlights.contains(&abs) {
                                    (Color32::from_rgb(80, 255, 255), Color32::from_rgb(0, 50, 50))
                                } else if inspector_highlights.contains(&abs) {
                                    (Color32::from_rgb(255, 150, 255), Color32::from_rgb(50, 0, 50))
                                } else {
                                    (Color32::from_rgb(220, 220, 220), Color32::TRANSPARENT)
                                }
                            };

                            // Hex display - use LayoutJob with click sensing when in edit mode
                            let mut job = egui::text::LayoutJob::default();
                            for (j, &original_byte) in data.iter().enumerate() {
                                let abs = byte_offset + j as u64;
                                let byte_val = get_byte(abs, original_byte);
                                let (fg, bg) = get_colors(abs);
                                let mut s = format!("{:02X} ", byte_val);
                                if j == 7 { s.push(' '); }
                                job.append(&s, 0.0, egui::TextFormat {
                                    font_id: FontId::monospace(13.0),
                                    color: fg,
                                    background: bg,
                                    ..Default::default()
                                });
                            }
                            // Pad remaining
                            for j in data.len()..16 {
                                let mut s = "   ".to_string();
                                if j == 7 { s.push(' '); }
                                job.append(&s, 0.0, egui::TextFormat {
                                    font_id: FontId::monospace(13.0),
                                    color: Color32::from_rgb(220, 220, 220),
                                    ..Default::default()
                                });
                            }

                            // Make hex clickable in edit mode
                            if edit_enabled {
                                let response = ui.add(egui::Label::new(job).sense(Sense::click()));
                                if response.clicked() {
                                    // Calculate which byte was clicked based on cursor position
                                    if let Some(pos) = response.interact_pointer_pos() {
                                        let relative_x = pos.x - response.rect.left();
                                        // Each byte takes ~24 pixels (2 hex chars + space at 8px each)
                                        // Plus extra space after byte 7
                                        let char_width = 8.0;
                                        let byte_width = char_width * 3.0; // "XX "
                                        let mut x = 0.0;
                                        for j in 0..data.len() {
                                            let next_x = x + byte_width + if j == 7 { char_width } else { 0.0 };
                                            if relative_x >= x && relative_x < next_x {
                                                clicked_offset = Some(byte_offset + j as u64);
                                                break;
                                            }
                                            x = next_x;
                                        }
                                    }
                                }
                                // Tooltip showing click hint
                                response.on_hover_text("Click a byte to edit");
                            } else {
                                ui.label(job);
                            }

                            // ASCII display
                            let mut ascii_job = egui::text::LayoutJob::default();
                            ascii_job.append("|", 0.0, egui::TextFormat {
                                font_id: FontId::monospace(13.0),
                                color: Color32::from_rgb(160, 200, 140),
                                ..Default::default()
                            });
                            for (j, &original_byte) in data.iter().enumerate() {
                                let abs = byte_offset + j as u64;
                                let byte_val = get_byte(abs, original_byte);
                                let ch = if byte_val.is_ascii_graphic() || byte_val == b' ' { byte_val as char } else { '.' };
                                let (fg, bg) = get_colors(abs);
                                // For ASCII, keep green tint when not highlighted/modified/selected
                                let fg = if is_highlighted(abs) || is_modified(abs) || is_selected(abs) {
                                    fg
                                } else {
                                    Color32::from_rgb(160, 200, 140)
                                };
                                ascii_job.append(&ch.to_string(), 0.0, egui::TextFormat {
                                    font_id: FontId::monospace(13.0),
                                    color: fg,
                                    background: bg,
                                    ..Default::default()
                                });
                            }
                            for _ in data.len()..16 {
                                ascii_job.append(" ", 0.0, egui::TextFormat {
                                    font_id: FontId::monospace(13.0),
                                    color: Color32::from_rgb(160, 200, 140),
                                    ..Default::default()
                                });
                            }
                            ascii_job.append("|", 0.0, egui::TextFormat {
                                font_id: FontId::monospace(13.0),
                                color: Color32::from_rgb(160, 200, 140),
                                ..Default::default()
                            });
                            ui.label(ascii_job);
                        });
                    }
                }
            });

        // Handle byte click outside the closure
        if let Some(offset) = clicked_offset {
            state.edit.selected_offset = Some(offset);
            state.edit.input_buffer.clear();
            // Pre-fill with current value
            if let Some(ref file) = state.file {
                if offset < file.mapped.len() {
                    let current = state.edit.get_edited_byte(offset)
                        .unwrap_or_else(|| file.mapped.slice(FileRegion::new(offset, 1))[0]);
                    state.edit.input_buffer = format!("{:02X}", current);
                }
            }
        }
    }

    /// Show file B in diff mode with synchronized scroll.
    /// Returns the scroll offset for synchronization.
    pub fn show_file_b(ui: &mut Ui, state: &mut AppState, scroll_offset: f32) -> f32 {
        let file_b = match &state.diff.file_b {
            Some(f) => f,
            None => {
                ui.centered_and_justified(|ui| {
                    ui.label("No file B loaded for comparison");
                });
                return 0.0;
            }
        };

        let file_len = file_b.mapped.len();
        let total_rows = file_len.div_ceil(BYTES_PER_ROW);

        // Rebuild diff highlights for the visible viewport
        {
            let vp_start = state.viewport.start;
            let vp_end = vp_start.saturating_add(BYTES_PER_ROW * 64).min(file_len);
            state.diff.rebuild_highlights_for_viewport(vp_start, vp_end);
        }

        let diff_highlights = state.diff.highlight_set.clone();
        let has_highlights = !diff_highlights.is_empty();

        let window_rows = if total_rows > MAX_DIRECT_ROWS {
            MAX_DIRECT_ROWS as usize
        } else {
            total_rows as usize
        };

        let coarse_offset = state.viewport.start;

        let scroll_output = ScrollArea::vertical()
            .id_salt("hex_panel_b")
            .auto_shrink([false, false])
            .vertical_scroll_offset(scroll_offset)
            .show_rows(ui, ROW_HEIGHT, window_rows, |ui, row_range| {
                let file = match &state.diff.file_b {
                    Some(f) => f,
                    None => return,
                };

                ui.style_mut().override_font_id = Some(FontId::monospace(13.0));

                for row_idx in row_range {
                    let byte_offset = coarse_offset + (row_idx as u64) * BYTES_PER_ROW;
                    if byte_offset >= file_len {
                        break;
                    }

                    let region = FileRegion::new(byte_offset, BYTES_PER_ROW);
                    let data = file.mapped.slice(region);

                    if !has_highlights {
                        let line = format_hex_line(byte_offset, data);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&line.offset).color(Color32::from_rgb(100, 140, 180)));
                            ui.label(RichText::new(&line.hex).color(Color32::from_rgb(220, 220, 220)));
                            ui.label(RichText::new(&line.ascii).color(Color32::from_rgb(160, 200, 140)));
                        });
                    } else {
                        // Highlighted path for diff
                        let line = format_hex_line(byte_offset, data);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&line.offset).color(Color32::from_rgb(100, 140, 180)));

                            let highlight_colors = |abs: u64| -> (Color32, Color32) {
                                if diff_highlights.contains(&abs) {
                                    (Color32::from_rgb(255, 100, 100), Color32::from_rgb(80, 0, 0))
                                } else {
                                    (Color32::from_rgb(220, 220, 220), Color32::TRANSPARENT)
                                }
                            };

                            // Hex with highlights
                            let mut job = egui::text::LayoutJob::default();
                            for (j, &b) in data.iter().enumerate() {
                                let abs = byte_offset + j as u64;
                                let (fg, bg) = highlight_colors(abs);
                                let mut s = format!("{:02X} ", b);
                                if j == 7 { s.push(' '); }
                                job.append(&s, 0.0, egui::TextFormat {
                                    font_id: FontId::monospace(13.0),
                                    color: fg,
                                    background: bg,
                                    ..Default::default()
                                });
                            }
                            for j in data.len()..16 {
                                let mut s = "   ".to_string();
                                if j == 7 { s.push(' '); }
                                job.append(&s, 0.0, egui::TextFormat {
                                    font_id: FontId::monospace(13.0),
                                    color: Color32::from_rgb(220, 220, 220),
                                    ..Default::default()
                                });
                            }
                            ui.label(job);

                            // ASCII with highlights
                            let mut ascii_job = egui::text::LayoutJob::default();
                            ascii_job.append("|", 0.0, egui::TextFormat {
                                font_id: FontId::monospace(13.0),
                                color: Color32::from_rgb(160, 200, 140),
                                ..Default::default()
                            });
                            for (j, &b) in data.iter().enumerate() {
                                let abs = byte_offset + j as u64;
                                let ch = if b.is_ascii_graphic() || b == b' ' { b as char } else { '.' };
                                let (fg, bg) = highlight_colors(abs);
                                let fg = if diff_highlights.contains(&abs) { fg } else { Color32::from_rgb(160, 200, 140) };
                                ascii_job.append(&ch.to_string(), 0.0, egui::TextFormat {
                                    font_id: FontId::monospace(13.0),
                                    color: fg,
                                    background: bg,
                                    ..Default::default()
                                });
                            }
                            for _ in data.len()..16 {
                                ascii_job.append(" ", 0.0, egui::TextFormat {
                                    font_id: FontId::monospace(13.0),
                                    color: Color32::from_rgb(160, 200, 140),
                                    ..Default::default()
                                });
                            }
                            ascii_job.append("|", 0.0, egui::TextFormat {
                                font_id: FontId::monospace(13.0),
                                color: Color32::from_rgb(160, 200, 140),
                                ..Default::default()
                            });
                            ui.label(ascii_job);
                        });
                    }
                }
            });

        scroll_output.state.offset.y
    }

    /// Show file A in diff mode with scroll offset tracking.
    /// Returns the scroll offset for synchronization.
    pub fn show_with_scroll(ui: &mut Ui, state: &mut AppState, scroll_offset: f32) -> f32 {
        if !state.has_file() {
            ui.label("Open a file to view its contents.");
            return 0.0;
        }

        let file_len = state.file_len();
        let total_rows = file_len.div_ceil(BYTES_PER_ROW);

        // Rebuild both search and diff highlights
        {
            let vp_start = state.viewport.start;
            let vp_end = vp_start.saturating_add(BYTES_PER_ROW * 64).min(file_len);
            state.search.rebuild_highlights_for_viewport(vp_start, vp_end);
            state.diff.rebuild_highlights_for_viewport(vp_start, vp_end);
        }

        let search_highlights = &state.search.highlight_set;
        let deep_scan_highlights = &state.deep_scan.highlight_set;
        let diff_highlights = state.diff.highlight_set.clone();
        let has_highlights = !search_highlights.is_empty() || !deep_scan_highlights.is_empty() || !diff_highlights.is_empty();

        let window_rows = if total_rows > MAX_DIRECT_ROWS {
            MAX_DIRECT_ROWS as usize
        } else {
            total_rows as usize
        };

        let coarse_offset = state.viewport.start;

        let scroll_output = ScrollArea::vertical()
            .id_salt("hex_panel_a")
            .auto_shrink([false, false])
            .vertical_scroll_offset(scroll_offset)
            .show_rows(ui, ROW_HEIGHT, window_rows, |ui, row_range| {
                let file = match &state.file {
                    Some(f) => f,
                    None => return,
                };

                ui.style_mut().override_font_id = Some(FontId::monospace(13.0));

                for row_idx in row_range {
                    let byte_offset = coarse_offset + (row_idx as u64) * BYTES_PER_ROW;
                    if byte_offset >= file_len {
                        break;
                    }

                    let region = FileRegion::new(byte_offset, BYTES_PER_ROW);
                    let data = file.mapped.slice(region);

                    let class_bg = state.classification.as_ref().and_then(|c| {
                        let block_idx = (byte_offset / 256) as usize;
                        c.get(block_idx).map(|&v| class_to_subtle_bg(v))
                    });

                    if !has_highlights {
                        let line = format_hex_line(byte_offset, data);
                        ui.horizontal(|ui| {
                            let offset_text = RichText::new(&line.offset)
                                .color(Color32::from_rgb(100, 140, 180))
                                .background_color(class_bg.unwrap_or(Color32::TRANSPARENT));
                            ui.label(offset_text);
                            ui.label(RichText::new(&line.hex).color(Color32::from_rgb(220, 220, 220)));
                            ui.label(RichText::new(&line.ascii).color(Color32::from_rgb(160, 200, 140)));
                        });
                    } else {
                        let line = format_hex_line(byte_offset, data);
                        ui.horizontal(|ui| {
                            let offset_text = RichText::new(&line.offset)
                                .color(Color32::from_rgb(100, 140, 180))
                                .background_color(class_bg.unwrap_or(Color32::TRANSPARENT));
                            ui.label(offset_text);

                            let highlight_colors = |abs: u64| -> (Color32, Color32) {
                                if search_highlights.contains(&abs) {
                                    (Color32::from_rgb(255, 255, 80), Color32::from_rgb(50, 50, 0))
                                } else if deep_scan_highlights.contains(&abs) {
                                    (Color32::from_rgb(80, 255, 255), Color32::from_rgb(0, 50, 50))
                                } else if diff_highlights.contains(&abs) {
                                    (Color32::from_rgb(255, 100, 100), Color32::from_rgb(80, 0, 0))
                                } else {
                                    (Color32::from_rgb(220, 220, 220), Color32::TRANSPARENT)
                                }
                            };

                            // Hex with highlights
                            let mut job = egui::text::LayoutJob::default();
                            for (j, &b) in data.iter().enumerate() {
                                let abs = byte_offset + j as u64;
                                let (fg, bg) = highlight_colors(abs);
                                let mut s = format!("{:02X} ", b);
                                if j == 7 { s.push(' '); }
                                job.append(&s, 0.0, egui::TextFormat {
                                    font_id: FontId::monospace(13.0),
                                    color: fg,
                                    background: bg,
                                    ..Default::default()
                                });
                            }
                            for j in data.len()..16 {
                                let mut s = "   ".to_string();
                                if j == 7 { s.push(' '); }
                                job.append(&s, 0.0, egui::TextFormat {
                                    font_id: FontId::monospace(13.0),
                                    color: Color32::from_rgb(220, 220, 220),
                                    ..Default::default()
                                });
                            }
                            ui.label(job);

                            // ASCII with highlights
                            let mut ascii_job = egui::text::LayoutJob::default();
                            ascii_job.append("|", 0.0, egui::TextFormat {
                                font_id: FontId::monospace(13.0),
                                color: Color32::from_rgb(160, 200, 140),
                                ..Default::default()
                            });
                            for (j, &b) in data.iter().enumerate() {
                                let abs = byte_offset + j as u64;
                                let ch = if b.is_ascii_graphic() || b == b' ' { b as char } else { '.' };
                                let (fg, bg) = highlight_colors(abs);
                                let fg = if search_highlights.contains(&abs) || deep_scan_highlights.contains(&abs) || diff_highlights.contains(&abs) {
                                    fg
                                } else {
                                    Color32::from_rgb(160, 200, 140)
                                };
                                ascii_job.append(&ch.to_string(), 0.0, egui::TextFormat {
                                    font_id: FontId::monospace(13.0),
                                    color: fg,
                                    background: bg,
                                    ..Default::default()
                                });
                            }
                            for _ in data.len()..16 {
                                ascii_job.append(" ", 0.0, egui::TextFormat {
                                    font_id: FontId::monospace(13.0),
                                    color: Color32::from_rgb(160, 200, 140),
                                    ..Default::default()
                                });
                            }
                            ascii_job.append("|", 0.0, egui::TextFormat {
                                font_id: FontId::monospace(13.0),
                                color: Color32::from_rgb(160, 200, 140),
                                ..Default::default()
                            });
                            ui.label(ascii_job);
                        });
                    }
                }
            });

        scroll_output.state.offset.y
    }

    /// Coarse slider for navigating large files (>128 MB).
    /// Returns the byte offset of the selected window start.
    fn show_coarse_slider(ui: &mut Ui, file_len: u64, state: &mut AppState) -> u64 {
        let max_offset = file_len.saturating_sub(MAX_DIRECT_ROWS * BYTES_PER_ROW);

        // Store coarse offset in viewport.start (aligned to row boundary)
        let mut offset = state.viewport.start.min(max_offset);

        ui.horizontal(|ui| {
            ui.label("Navigate:");
            let slider_val = &mut (offset as f64);
            let response = ui.add(
                egui::Slider::new(slider_val, 0.0..=(max_offset as f64))
                    .text("offset")
                    .custom_formatter(|v, _| format_offset(v as u64))
            );
            if response.changed() {
                offset = (*slider_val as u64 / BYTES_PER_ROW) * BYTES_PER_ROW;
            }
        });

        // Show position indicator
        let pct = if file_len > 0 {
            offset as f64 / file_len as f64 * 100.0
        } else {
            0.0
        };
        ui.label(format!(
            "{} / {} ({:.1}%)",
            format_offset(offset),
            format_offset(file_len),
            pct,
        ));

        state.viewport.start = offset;
        offset
    }

    /// Handle keyboard navigation. Must be called each frame.
    fn handle_keyboard(ui: &mut Ui, state: &mut AppState, file_len: u64) {
        // Don't steal keys when a text field has focus
        if ui.memory(|m| m.focused().is_some()) {
            return;
        }

        let page_bytes = BYTES_PER_ROW * 32; // ~32 rows per page

        ui.input(|i| {
            // Ctrl+G: open "Go to offset" dialog
            if i.modifiers.ctrl && i.key_pressed(egui::Key::G) {
                state.goto_open = true;
                state.goto_text.clear();
            }

            // Ctrl+E: toggle edit mode
            if i.modifiers.ctrl && i.key_pressed(egui::Key::E) {
                if state.edit.enabled {
                    if state.edit.has_changes() {
                        state.edit.save_dialog_open = true;
                    } else {
                        state.edit.clear();
                    }
                } else {
                    state.edit.confirm_dialog_open = true;
                }
            }

            // Escape: deselect byte in edit mode
            if i.key_pressed(egui::Key::Escape) && state.edit.selected_offset.is_some() {
                state.edit.selected_offset = None;
                state.edit.input_buffer.clear();
            }

            // Page Down
            if i.key_pressed(egui::Key::PageDown) {
                state.viewport.start = state.viewport.start
                    .saturating_add(page_bytes)
                    .min(file_len.saturating_sub(BYTES_PER_ROW));
            }
            // Page Up
            if i.key_pressed(egui::Key::PageUp) {
                state.viewport.start = state.viewport.start.saturating_sub(page_bytes);
            }
            // Home
            if i.key_pressed(egui::Key::Home) {
                state.viewport.start = 0;
            }
            // End
            if i.key_pressed(egui::Key::End) {
                state.viewport.start = (file_len.saturating_sub(page_bytes) / BYTES_PER_ROW) * BYTES_PER_ROW;
            }
            // Arrow Down
            if i.key_pressed(egui::Key::ArrowDown) {
                state.viewport.start = state.viewport.start
                    .saturating_add(BYTES_PER_ROW)
                    .min(file_len.saturating_sub(BYTES_PER_ROW));
            }
            // Arrow Up
            if i.key_pressed(egui::Key::ArrowUp) {
                state.viewport.start = state.viewport.start.saturating_sub(BYTES_PER_ROW);
            }
        });

        // "Go to offset" modal window
        if state.goto_open {
            Self::show_goto_dialog(ui, state, file_len);
        }
    }

    /// Show edit mode toolbar.
    fn show_edit_toolbar(ui: &mut Ui, state: &mut AppState) {
        ui.horizontal(|ui| {
            // Edit mode toggle
            if state.edit.enabled {
                // Warning indicator when in edit mode
                ui.label(RichText::new("EDIT MODE").color(Color32::from_rgb(255, 100, 100)).strong());

                ui.separator();

                // Show pending edit count
                let edit_count = state.edit.edit_count();
                if edit_count > 0 {
                    ui.label(RichText::new(format!("{} byte(s) modified", edit_count))
                        .color(Color32::from_rgb(255, 200, 100)));

                    if ui.button("Save").clicked() {
                        state.edit.save_dialog_open = true;
                    }

                    if ui.button("Discard").clicked() {
                        state.edit.undo_all();
                        state.edit.status_message = Some(("Changes discarded".to_string(), false));
                    }
                }

                ui.separator();

                if ui.button("Exit Edit Mode").clicked() {
                    if state.edit.has_changes() {
                        // Show save dialog if there are unsaved changes
                        state.edit.save_dialog_open = true;
                    } else {
                        state.edit.clear();
                    }
                }

                // Byte editor (when a byte is selected)
                if let Some(offset) = state.edit.selected_offset {
                    ui.separator();
                    ui.label(format!("Offset 0x{:X}:", offset));

                    let response = ui.add(
                        egui::TextEdit::singleline(&mut state.edit.input_buffer)
                            .desired_width(30.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("XX")
                    );

                    // Auto-focus when first selected
                    if response.gained_focus() || state.edit.input_buffer.is_empty() {
                        response.request_focus();
                    }

                    // Apply edit on Enter or when 2 valid hex chars entered
                    let should_apply = response.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        || state.edit.input_buffer.len() == 2
                            && state.edit.input_buffer.chars().all(|c| c.is_ascii_hexdigit());

                    if should_apply {
                        if let Ok(new_byte) = u8::from_str_radix(&state.edit.input_buffer, 16) {
                            // Get original byte
                            if let Some(ref file) = state.file {
                                if offset < file.mapped.len() {
                                    let original = file.mapped.slice(FileRegion::new(offset, 1))[0];
                                    state.edit.set_byte(offset, original, new_byte);
                                    state.edit.status_message = Some(
                                        (format!("Set 0x{:X} = {:02X}", offset, new_byte), false)
                                    );
                                }
                            }
                            state.edit.input_buffer.clear();
                            state.edit.selected_offset = None;
                        }
                    }

                    if ui.button("Cancel").clicked() {
                        state.edit.selected_offset = None;
                        state.edit.input_buffer.clear();
                    }
                }
            } else {
                // Show button to enable edit mode
                if ui.button("Enable Edit Mode").clicked() {
                    state.edit.confirm_dialog_open = true;
                }
                ui.weak("(Ctrl+E)");
            }

            // Status message
            if let Some((msg, is_error)) = &state.edit.status_message {
                ui.separator();
                let color = if *is_error { Color32::RED } else { Color32::from_rgb(100, 200, 100) };
                ui.label(RichText::new(msg).color(color).small());
            }
        });
    }

    /// Show edit mode confirmation and save dialogs.
    fn show_edit_dialogs(ui: &mut Ui, state: &mut AppState) {
        // Enable edit mode confirmation dialog (SAFETY WARNING)
        if state.edit.confirm_dialog_open {
            egui::Window::new("Enable Edit Mode")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.vertical_centered(|ui| {
                        // Big warning icon/text
                        ui.label(RichText::new("WARNING").size(24.0).color(Color32::from_rgb(255, 100, 100)).strong());

                        ui.add_space(10.0);

                        ui.label(RichText::new("You are about to enable HEX EDITING mode.")
                            .color(Color32::from_rgb(255, 200, 100)));

                        ui.add_space(10.0);

                        egui::Frame::none()
                            .fill(Color32::from_rgb(60, 30, 30))
                            .inner_margin(10.0)
                            .rounding(5.0)
                            .show(ui, |ui| {
                                ui.label(RichText::new("DANGER: This operation can PERMANENTLY CORRUPT your file!")
                                    .color(Color32::from_rgb(255, 150, 150)));

                                ui.add_space(5.0);

                                ui.label("Modifications are written DIRECTLY to the file on disk.");
                                ui.label("There is NO automatic backup.");
                                ui.label("Incorrect edits can render the file unusable.");
                            });

                        ui.add_space(10.0);

                        ui.label("Make a backup of your file before proceeding.");

                        ui.add_space(15.0);

                        ui.horizontal(|ui| {
                            if ui.add(
                                egui::Button::new(RichText::new("I understand, enable editing").color(Color32::from_rgb(255, 100, 100)))
                            ).clicked() {
                                state.edit.enabled = true;
                                state.edit.confirm_dialog_open = false;
                                state.edit.status_message = Some(("Edit mode enabled - BE CAREFUL!".to_string(), true));
                            }

                            if ui.button("Cancel").clicked() {
                                state.edit.confirm_dialog_open = false;
                            }
                        });
                    });
                });
        }

        // Save confirmation dialog
        if state.edit.save_dialog_open {
            egui::Window::new("Save Changes")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    let edit_count = state.edit.edit_count();

                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Save Changes?").size(18.0).strong());

                        ui.add_space(10.0);

                        ui.label(format!("You have {} modified byte(s).", edit_count));

                        ui.add_space(5.0);

                        egui::Frame::none()
                            .fill(Color32::from_rgb(60, 40, 20))
                            .inner_margin(10.0)
                            .rounding(5.0)
                            .show(ui, |ui| {
                                ui.label(RichText::new("This will PERMANENTLY modify the file on disk!")
                                    .color(Color32::from_rgb(255, 200, 100)));
                            });

                        ui.add_space(15.0);

                        ui.horizontal(|ui| {
                            if ui.add(
                                egui::Button::new(RichText::new("Save").color(Color32::from_rgb(255, 100, 100)))
                            ).clicked() {
                                // Perform save
                                if let Some(ref file) = state.file {
                                    match state.edit.save_to_file(&file.path) {
                                        Ok(count) => {
                                            state.edit.status_message = Some(
                                                (format!("Saved {} byte(s) to file", count), false)
                                            );
                                            // Note: The mmap will still show old data until file is re-opened
                                            // We invalidate caches to force recomputation
                                            state.entropy = None;
                                            state.classification = None;
                                            state.cached_entropy_stats = None;
                                            state.cached_class_counts = None;
                                        }
                                        Err(e) => {
                                            state.edit.status_message = Some(
                                                (format!("Save failed: {}", e), true)
                                            );
                                        }
                                    }
                                }
                                state.edit.save_dialog_open = false;
                            }

                            if ui.button("Discard").clicked() {
                                state.edit.undo_all();
                                state.edit.save_dialog_open = false;
                                state.edit.status_message = Some(("Changes discarded".to_string(), false));
                            }

                            if ui.button("Cancel").clicked() {
                                state.edit.save_dialog_open = false;
                            }
                        });
                    });
                });
        }
    }

    /// Show the "Go to offset" dialog (Ctrl+G).
    fn show_goto_dialog(ui: &mut Ui, state: &mut AppState, file_len: u64) {
        egui::Window::new("Go to offset")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.label("Enter hex offset (e.g. 0xFF00 or FF00):");
                let response = ui.text_edit_singleline(&mut state.goto_text);

                // Auto-focus the text field
                if response.gained_focus() || state.goto_text.is_empty() {
                    response.request_focus();
                }

                ui.horizontal(|ui| {
                    let enter = response.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter));

                    if ui.button("Go").clicked() || enter {
                        if let Some(offset) = parse_offset(&state.goto_text) {
                            let aligned = (offset.min(file_len.saturating_sub(1)) / BYTES_PER_ROW) * BYTES_PER_ROW;
                            state.viewport.start = aligned;
                            state.goto_open = false;
                        }
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        state.goto_open = false;
                    }
                });
            });
    }
}

/// Parse an offset string: "0xFF00", "FF00", "1024" (decimal).
fn parse_offset(input: &str) -> Option<u64> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else if s.chars().all(|c| c.is_ascii_hexdigit()) && s.chars().any(|c| c.is_ascii_alphabetic()) {
        // Contains letters → treat as hex
        u64::from_str_radix(s, 16).ok()
    } else {
        // Try decimal first, then hex
        s.parse::<u64>().ok().or_else(|| u64::from_str_radix(s, 16).ok())
    }
}

/// A formatted hex line split into its three visual parts.
pub struct HexLine {
    pub offset: String,
    pub hex: String,
    pub ascii: String,
}

/// Format one row of hex output.
pub fn format_hex_line(byte_offset: u64, data: &[u8]) -> HexLine {
    // Offset column
    let offset = format!("{:08X}  ", byte_offset);

    // Hex column
    let mut hex = String::with_capacity(50);
    for (j, &b) in data.iter().enumerate() {
        hex.push_str(&format!("{:02X} ", b));
        if j == 7 {
            hex.push(' ');
        }
    }
    // Pad if less than 16 bytes
    for j in data.len()..16 {
        hex.push_str("   ");
        if j == 7 {
            hex.push(' ');
        }
    }

    // ASCII column
    let mut ascii = String::with_capacity(18);
    ascii.push('|');
    for &b in data {
        if b.is_ascii_graphic() || b == b' ' {
            ascii.push(b as char);
        } else {
            ascii.push('.');
        }
    }
    // Pad
    for _ in data.len()..16 {
        ascii.push(' ');
    }
    ascii.push('|');

    HexLine { offset, hex, ascii }
}

/// Format a byte offset for display.
pub fn format_offset(offset: u64) -> String {
    if offset >= 1 << 30 {
        format!("0x{:X} ({:.1} GB)", offset, offset as f64 / (1u64 << 30) as f64)
    } else if offset >= 1 << 20 {
        format!("0x{:X} ({:.1} MB)", offset, offset as f64 / (1u64 << 20) as f64)
    } else {
        format!("0x{:X}", offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_hex_line_full_row() {
        let data: Vec<u8> = (0x00..=0x0F).collect();
        let line = format_hex_line(0, &data);

        assert_eq!(line.offset, "00000000  ");
        assert_eq!(
            line.hex,
            "00 01 02 03 04 05 06 07  08 09 0A 0B 0C 0D 0E 0F "
        );
        assert_eq!(line.ascii, "|................|");
    }

    #[test]
    fn format_hex_line_partial_row() {
        let data = b"Hello";
        let line = format_hex_line(0x100, data);

        assert_eq!(line.offset, "00000100  ");
        // "Hello" = 48 65 6C 6C 6F then padding
        assert!(line.hex.starts_with("48 65 6C 6C 6F "));
        assert!(line.ascii.starts_with("|Hello"));
        assert!(line.ascii.ends_with('|'));
        assert_eq!(line.ascii.len(), 18); // |16 chars|
    }

    #[test]
    fn format_hex_line_printable_ascii() {
        let data = b"ABCDEFGHIJKLMNOP";
        let line = format_hex_line(0, data);
        assert_eq!(line.ascii, "|ABCDEFGHIJKLMNOP|");
    }

    #[test]
    fn format_hex_line_at_large_offset() {
        let data = vec![0xFFu8; 16];
        let line = format_hex_line(0xDEAD_BEEF, &data);
        assert_eq!(line.offset, "DEADBEEF  ");
    }

    #[test]
    fn format_hex_line_separator_at_byte_8() {
        let data = vec![0xAAu8; 16];
        let line = format_hex_line(0, &data);
        // Should have double space between byte 7 and byte 8
        assert!(line.hex.contains("AA  AA"));
    }

    #[test]
    fn viewport_rows_small_file() {
        let file_len: u64 = 256;
        let total_rows = file_len.div_ceil(BYTES_PER_ROW);
        assert_eq!(total_rows, 16);
    }

    #[test]
    fn viewport_rows_exact_boundary() {
        let file_len: u64 = 16 * 100;
        let total_rows = file_len.div_ceil(BYTES_PER_ROW);
        assert_eq!(total_rows, 100);
    }

    #[test]
    fn viewport_rows_large_file() {
        // 1 TB file
        let file_len: u64 = 1u64 << 40;
        let total_rows = file_len.div_ceil(BYTES_PER_ROW);
        // Should be handled by coarse slider
        assert!(total_rows > MAX_DIRECT_ROWS);
    }

    #[test]
    fn format_offset_small() {
        assert_eq!(format_offset(0), "0x0");
        assert_eq!(format_offset(0xFF), "0xFF");
    }

    #[test]
    fn format_offset_large() {
        let offset = 1u64 << 30; // 1 GB
        let s = format_offset(offset);
        assert!(s.contains("GB"));
    }

    #[test]
    fn parse_offset_hex_prefix() {
        assert_eq!(parse_offset("0xFF00"), Some(0xFF00));
        assert_eq!(parse_offset("0X1A2B"), Some(0x1A2B));
    }

    #[test]
    fn parse_offset_hex_no_prefix() {
        assert_eq!(parse_offset("DEADBEEF"), Some(0xDEADBEEF));
        assert_eq!(parse_offset("ff"), Some(0xFF));
    }

    #[test]
    fn parse_offset_decimal() {
        assert_eq!(parse_offset("1024"), Some(1024));
        assert_eq!(parse_offset("0"), Some(0));
    }

    #[test]
    fn parse_offset_empty() {
        assert_eq!(parse_offset(""), None);
        assert_eq!(parse_offset("  "), None);
    }
}
