//! Script Console window UI.

use egui::{Context, Color32, RichText, ScrollArea, TextEdit, FontId, Vec2};
use crate::state::AppState;
use crate::scripting::ScriptState;
use crate::syntax_highlight::highlight_rhai;
use tv_core::FileRegion;

/// Script Console window.
pub struct ScriptWindow;

/// Height ratio for output panel (0.0 to 1.0).
static mut OUTPUT_RATIO: f32 = 0.5;

impl ScriptWindow {
    pub fn show(
        ctx: &Context,
        state: &mut AppState,
        script: &mut ScriptState,
        visible: &mut bool,
    ) {
        if !*visible {
            return;
        }

        egui::Window::new("Script Console")
            .open(visible)
            .default_size([800.0, 600.0])
            .min_size([400.0, 300.0])
            .resizable(true)
            .scroll(false)
            .show(ctx, |ui| {
                Self::show_contents(ui, state, script);
            });
    }

    fn show_contents(ui: &mut egui::Ui, state: &mut AppState, script: &mut ScriptState) {
        // Update context with current file data
        if let Some(ref file) = state.file {
            let len = file.mapped.len().min(10 * 1024 * 1024) as u64; // Limit to 10MB for scripts
            let data = file.mapped.slice(FileRegion::new(0, len)).to_vec();
            script.set_context(data, state.viewport.start);
        }

        // Top toolbar
        ui.horizontal(|ui| {
            if ui.button("Run (Ctrl+Enter)").clicked() {
                Self::execute_script(state, script);
            }

            if ui.button("Clear").clicked() {
                script.output.clear();
            }

            ui.separator();

            ui.checkbox(&mut script.repl_mode, "REPL mode")
                .on_hover_text("Execute on Enter (single line)");

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if script.last_error.is_some() {
                    ui.label(RichText::new("Error").color(Color32::RED));
                }
            });
        });

        ui.separator();

        // Get the output ratio safely
        let output_ratio = unsafe { OUTPUT_RATIO };
        let available = ui.available_height() - 30.0; // Reserve space for splitter and status
        let output_height = (available * output_ratio).max(80.0);
        let input_height = (available * (1.0 - output_ratio)).max(100.0);

        // Output area
        ui.label(RichText::new("Output:").small().weak());
        egui::Frame::none()
            .fill(Color32::from_rgb(20, 20, 25))
            .inner_margin(8.0)
            .rounding(4.0)
            .show(ui, |ui| {
                ScrollArea::vertical()
                    .id_salt("script_output")
                    .max_height(output_height)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.style_mut().override_font_id = Some(FontId::monospace(12.0));

                        for line in &script.output {
                            let color = if line.starts_with('>') {
                                Color32::from_rgb(100, 200, 255) // Command echo
                            } else if line.starts_with("Error") || line.starts_with("error") {
                                Color32::from_rgb(255, 100, 100) // Errors
                            } else if line.starts_with("=>") {
                                Color32::from_rgb(100, 255, 100) // Results
                            } else if line.starts_with("===") {
                                Color32::from_rgb(255, 200, 100) // Headers
                            } else {
                                Color32::from_rgb(200, 200, 200) // Normal
                            };
                            ui.label(RichText::new(line).color(color));
                        }
                    });
            });

        // Resizable splitter
        let splitter_response = ui.allocate_response(
            Vec2::new(ui.available_width(), 8.0),
            egui::Sense::drag(),
        );

        // Draw splitter handle
        let splitter_rect = splitter_response.rect;
        ui.painter().rect_filled(
            splitter_rect,
            2.0,
            if splitter_response.hovered() || splitter_response.dragged() {
                Color32::from_rgb(100, 150, 200)
            } else {
                Color32::from_rgb(60, 60, 70)
            },
        );

        // Handle splitter drag
        if splitter_response.dragged() {
            let delta = splitter_response.drag_delta().y;
            unsafe {
                OUTPUT_RATIO = (OUTPUT_RATIO + delta / available).clamp(0.2, 0.8);
            }
        }

        // Change cursor on hover
        if splitter_response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }

        // Input area with examples sidebar
        ui.horizontal_top(|ui| {
            // Examples sidebar
            ui.vertical(|ui| {
                ui.set_min_width(90.0);
                ui.set_max_width(90.0);
                ui.label(RichText::new("Examples").small().strong());

                let mut clicked_example = None;
                ScrollArea::vertical()
                    .id_salt("script_examples")
                    .max_height(input_height - 50.0)
                    .show(ui, |ui| {
                        for (idx, (name, _)) in script.examples.iter().enumerate() {
                            if ui.small_button(*name).clicked() {
                                clicked_example = Some(idx);
                            }
                        }
                    });
                if let Some(idx) = clicked_example {
                    script.load_example(idx);
                }

                ui.add_space(4.0);
                if ui.small_button("help").clicked() {
                    script.script_text = "help".to_string();
                    Self::execute_script(state, script);
                }
            });

            ui.separator();

            // Script editor with line numbers
            ui.vertical(|ui| {
                let line_count = script.script_text.lines().count().max(1);
                let editor_height = input_height - 30.0;

                // Build line numbers string
                let line_numbers: String = (1..=line_count.max(10))
                    .map(|i| format!("{:4}\n", i))
                    .collect();

                egui::Frame::none()
                    .fill(Color32::from_rgb(25, 25, 30))
                    .inner_margin(4.0)
                    .rounding(4.0)
                    .show(ui, |ui| {
                        ScrollArea::vertical()
                            .id_salt("script_editor_scroll")
                            .max_height(editor_height)
                            .show(ui, |ui| {
                                ui.horizontal_top(|ui| {
                                    // Line numbers (non-editable)
                                    ui.add(
                                        TextEdit::multiline(&mut line_numbers.clone())
                                            .font(FontId::monospace(13.0))
                                            .text_color(Color32::from_rgb(80, 80, 100))
                                            .desired_width(35.0)
                                            .frame(false)
                                            .interactive(false)
                                    );

                                    ui.add_space(4.0);

                                    // Vertical separator line
                                    let rect = ui.available_rect_before_wrap();
                                    ui.painter().vline(
                                        rect.left(),
                                        rect.top()..=rect.bottom(),
                                        egui::Stroke::new(1.0, Color32::from_rgb(50, 50, 60))
                                    );

                                    ui.add_space(4.0);

                                    // Script text editor with syntax highlighting
                                    let font_id = FontId::monospace(13.0);
                                    let mut layouter = |ui: &egui::Ui, text: &str, _wrap_width: f32| {
                                        let layout_job = highlight_rhai(text, FontId::monospace(13.0));
                                        ui.fonts(|f| f.layout_job(layout_job))
                                    };

                                    let response = ui.add(
                                        TextEdit::multiline(&mut script.script_text)
                                            .font(font_id)
                                            .desired_width(ui.available_width())
                                            .desired_rows(line_count.max(10))
                                            .frame(false)
                                            .lock_focus(true)
                                            .layouter(&mut layouter)
                                    );

                                    // Handle keyboard shortcuts
                                    if response.has_focus() {
                                        ui.input(|i| {
                                            if i.key_pressed(egui::Key::ArrowUp) && script.script_text.is_empty() {
                                                script.history_up();
                                            }
                                            if i.key_pressed(egui::Key::ArrowDown) {
                                                script.history_down();
                                            }
                                        });

                                        if ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Enter)) {
                                            Self::execute_script(state, script);
                                        }

                                        if script.repl_mode
                                            && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                            && !script.script_text.contains('\n')
                                        {
                                            Self::execute_script(state, script);
                                            script.script_text.clear();
                                        }
                                    }
                                });
                            });
                    });

                // Status bar
                ui.horizontal(|ui| {
                    let lines = script.script_text.lines().count().max(1);
                    ui.weak(format!("Ln {}, {} chars", lines, script.script_text.len()));

                    if let Some(ref err) = script.last_error {
                        ui.separator();
                        ui.label(RichText::new(err).color(Color32::RED).small());
                    }
                });
            });
        });
    }

    fn execute_script(state: &mut AppState, script: &mut ScriptState) {
        // Execute the script
        let _ = script.execute();

        // Apply any pending edits to the edit buffer
        let edits = script.take_edits();
        let edits_count = edits.len();
        if !edits.is_empty() {
            if !state.edit.enabled {
                // Auto-enable edit mode (with warning in output)
                script.output.push("Warning: Edit mode auto-enabled for script writes.".to_string());
                script.output.push("Use 'Save' to apply changes to file.".to_string());
                state.edit.enabled = true;
            }

            for (offset, value) in edits {
                if let Some(ref file) = state.file {
                    if offset < file.mapped.len() {
                        let original = file.mapped.slice(FileRegion::new(offset, 1))[0];
                        state.edit.set_byte(offset, original, value);
                    }
                }
            }
            script.output.push(format!("Applied {} byte edit(s) to buffer.", edits_count));
        }

        // Handle goto requests
        if let Some(offset) = script.take_goto() {
            state.viewport.start = (offset / 16) * 16;
            script.output.push(format!("Navigated to 0x{:X}", offset));
        }

        // Handle search results
        let search_results = script.take_search_results();
        if !search_results.is_empty() {
            state.search.results = Some(search_results.clone());
            state.search.selected_result = Some(0);
            state.search.rebuild_highlights();
            // Navigate to first result
            if let Some(&first) = search_results.first() {
                state.viewport.start = (first / 16) * 16;
            }
        }
    }
}
