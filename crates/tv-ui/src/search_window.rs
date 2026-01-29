use egui::Context;
use crate::state::{AppState, parse_hex_pattern};
use crate::hex_panel::format_offset;

/// Floating window for pattern search.
pub struct SearchWindow;

/// Maximum results shown in the scrollable list.
const MAX_VISIBLE_RESULTS: usize = 10_000;
/// Row height for virtual scroll.
const RESULT_ROW_HEIGHT: f32 = 18.0;

impl SearchWindow {
    pub fn show(ctx: &Context, state: &mut AppState, visible: &mut bool) {
        if !*visible {
            return;
        }

        egui::Window::new("Search")
            .open(visible)
            .default_size([320.0, 400.0])
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

        // Hex input
        ui.label("Hex pattern (max 16 bytes):");
        let response = ui.text_edit_singleline(&mut state.search.query_text);

        // Show parse preview
        match parse_hex_pattern(&state.search.query_text) {
            Ok(bytes) => {
                ui.horizontal(|ui| {
                    ui.label(format!("{} bytes:", bytes.len()));
                    ui.code(format!("{:02X?}", bytes));
                });
            }
            Err(e) if !state.search.query_text.trim().is_empty() => {
                ui.colored_label(egui::Color32::from_rgb(255, 100, 100), e);
            }
            _ => {}
        }

        ui.add_space(4.0);

        // Search button
        let can_search = !state.search.searching
            && parse_hex_pattern(&state.search.query_text).is_ok();

        ui.horizontal(|ui| {
            let search_clicked = ui.add_enabled(can_search, egui::Button::new("Search")).clicked();
            let enter_pressed = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            if (search_clicked || enter_pressed) && can_search {
                if let Ok(bytes) = parse_hex_pattern(&state.search.query_text) {
                    state.search.pattern = Some(bytes);
                    state.search.searching = true;
                    state.search.results = None;
                    state.search.selected_result = None;
                    state.search.search_duration_ms = None;
                }
            }

            if state.search.results.is_some() {
                if ui.button("Clear").clicked() {
                    state.search.results = None;
                    state.search.selected_result = None;
                    state.search.search_duration_ms = None;
                    state.search.rebuild_highlights();
                }
            }
        });

        if state.search.searching {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Searching...");
            });
        }

        ui.add_space(8.0);

        // Results
        Self::show_results(ui, state);
    }

    fn show_results(ui: &mut egui::Ui, state: &mut AppState) {
        let results_info = state.search.results.as_ref().map(|r| {
            let count = r.len();
            let first_offset = r.first().copied();
            let last_offset = r.last().copied();
            let sel = state.search.selected_result.unwrap_or(0);
            let prev_offset = if sel > 0 { r.get(sel - 1).copied() } else { None };
            let next_offset = r.get(sel + 1).copied();
            (count, first_offset, last_offset, prev_offset, next_offset, sel)
        });

        if let Some((count, first_offset, last_offset, prev_offset, next_offset, sel)) = results_info {
            ui.separator();

            // Stats
            ui.horizontal(|ui| {
                ui.strong(format!("{} match(es)", count));
                if let Some(ms) = state.search.search_duration_ms {
                    ui.weak(format!("in {:.1} ms", ms));
                }
            });

            if count == 0 {
                ui.label("No matches found.");
                return;
            }

            // Memory usage
            let mem_bytes = count * std::mem::size_of::<u64>();
            ui.weak(format!("Results memory: {}", format_memory(mem_bytes)));

            // Navigation buttons
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("|<").on_hover_text("First (Home)").clicked() {
                    if let Some(offset) = first_offset {
                        state.search.selected_result = Some(0);
                        state.viewport.start = (offset / 16) * 16;
                        state.search.rebuild_highlights();
                    }
                }
                if ui.button("<").on_hover_text("Previous").clicked() {
                    if let Some(offset) = prev_offset {
                        state.search.selected_result = Some(sel - 1);
                        state.viewport.start = (offset / 16) * 16;
                        state.search.rebuild_highlights();
                    }
                }
                ui.label(format!("{} / {}", sel + 1, count));
                if ui.button(">").on_hover_text("Next").clicked() {
                    if let Some(offset) = next_offset {
                        state.search.selected_result = Some(sel + 1);
                        state.viewport.start = (offset / 16) * 16;
                        state.search.rebuild_highlights();
                    }
                }
                if ui.button(">|").on_hover_text("Last (End)").clicked() {
                    if let Some(offset) = last_offset {
                        state.search.selected_result = Some(count - 1);
                        state.viewport.start = (offset / 16) * 16;
                        state.search.rebuild_highlights();
                    }
                }
            });

            ui.add_space(4.0);

            // Results list
            let display_count = count.min(MAX_VISIBLE_RESULTS);
            let visible_offsets: Vec<(usize, u64)> = state.search.results.as_ref()
                .map(|r| r.iter().take(display_count).copied().enumerate().collect())
                .unwrap_or_default();

            egui::ScrollArea::vertical()
                .max_height(250.0)
                .show_rows(ui, RESULT_ROW_HEIGHT, display_count, |ui, row_range| {
                    for i in row_range {
                        if let Some(&(idx, offset)) = visible_offsets.get(i) {
                            let selected = state.search.selected_result == Some(idx);
                            let text = format!("#{}: {}", idx + 1, format_offset(offset));
                            if ui.selectable_label(selected, text).clicked() {
                                state.search.selected_result = Some(idx);
                                state.viewport.start = (offset / 16) * 16;
                                state.search.rebuild_highlights();
                            }
                        }
                    }
                });

            if count > MAX_VISIBLE_RESULTS {
                ui.weak(format!(
                    "Showing first {} of {} results.",
                    MAX_VISIBLE_RESULTS, count
                ));
            }
        }
    }
}

fn format_memory(bytes: usize) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}
