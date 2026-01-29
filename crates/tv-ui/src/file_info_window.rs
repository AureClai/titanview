use egui::{Context, Color32};
use tv_core::BlockClass;
use crate::state::AppState;

/// Floating window for file metadata and analysis summary.
pub struct FileInfoWindow;

impl FileInfoWindow {
    pub fn show(ctx: &Context, state: &mut AppState, visible: &mut bool) {
        if !*visible {
            return;
        }

        egui::Window::new("File Info")
            .open(visible)
            .default_size([300.0, 400.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                Self::show_contents(ui, state);
            });
    }

    fn show_contents(ui: &mut egui::Ui, state: &mut AppState) {
        if !state.has_file() {
            ui.label("No file loaded.");
            return;
        }

        // File metadata section
        ui.heading("Metadata");
        egui::Grid::new("file_info_grid")
            .num_columns(2)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                ui.strong("Name:");
                ui.label(state.file_name());
                ui.end_row();

                ui.strong("Path:");
                ui.label(state.file_path_display());
                ui.end_row();

                ui.strong("Size:");
                ui.label(format_size(state.file_len()));
                ui.end_row();
            });

        ui.add_space(12.0);

        // Entropy section
        if let Some(stats) = state.cached_entropy_stats {
            ui.heading("Entropy Analysis");
            ui.horizontal(|ui| {
                ui.strong("Average:");
                let entropy_color = entropy_to_color(stats.avg);
                ui.colored_label(entropy_color, format!("{:.2} bits/byte", stats.avg));
            });
            ui.label(format!("{} blocks analyzed (256 bytes each)", stats.block_count));

            // Entropy bar
            let normalized = stats.avg / 8.0;
            ui.add(egui::ProgressBar::new(normalized)
                .text(format!("{:.1}%", normalized * 100.0)));

            ui.add_space(8.0);
        }

        // Classification section
        if let Some(counts) = state.cached_class_counts {
            if let Some(ref classification) = state.classification {
                ui.heading("Block Classification");

                let total = classification.len() as f32;

                egui::Grid::new("class_grid")
                    .num_columns(3)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        ui.strong("Type");
                        ui.strong("Blocks");
                        ui.strong("Percent");
                        ui.end_row();

                        for class_id in 0..5u8 {
                            let class = BlockClass::from_u8(class_id);
                            let count = counts[class_id as usize];
                            if count > 0 {
                                let pct = count as f32 / total * 100.0;
                                let color = class_to_color(class_id);
                                ui.colored_label(color, class.label());
                                ui.label(format!("{}", count));
                                ui.label(format!("{:.1}%", pct));
                                ui.end_row();
                            }
                        }
                    });

                // Visual bar chart
                ui.add_space(8.0);
                Self::draw_class_bar(ui, &counts, total);
            }
        }
    }

    fn draw_class_bar(ui: &mut egui::Ui, counts: &[u32; 5], total: f32) {
        let desired_size = egui::vec2(ui.available_width(), 24.0);
        let (rect, _response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

        if total <= 0.0 {
            return;
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, Color32::from_gray(40));

        let mut x = rect.left();
        for (class_id, &count) in counts.iter().enumerate() {
            if count == 0 {
                continue;
            }
            let width = (count as f32 / total) * rect.width();
            let color = class_to_color(class_id as u8);
            let bar_rect = egui::Rect::from_min_size(
                egui::pos2(x, rect.top()),
                egui::vec2(width, rect.height()),
            );
            painter.rect_filled(bar_rect, 0.0, color);
            x += width;
        }

        // Border
        painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, Color32::from_gray(80)));
    }
}

/// Format bytes into a human-readable string.
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    if bytes >= TB {
        format!("{:.2} TB ({} bytes)", bytes as f64 / TB as f64, bytes)
    } else if bytes >= GB {
        format!("{:.2} GB ({} bytes)", bytes as f64 / GB as f64, bytes)
    } else if bytes >= MB {
        format!("{:.2} MB ({} bytes)", bytes as f64 / MB as f64, bytes)
    } else if bytes >= KB {
        format!("{:.2} KB ({} bytes)", bytes as f64 / KB as f64, bytes)
    } else {
        format!("{} bytes", bytes)
    }
}

/// Map entropy value to a color (green=low, yellow=medium, red=high).
fn entropy_to_color(entropy: f32) -> Color32 {
    if entropy < 4.0 {
        Color32::from_rgb(100, 200, 100) // Green - structured
    } else if entropy < 6.0 {
        Color32::from_rgb(200, 200, 100) // Yellow - mixed
    } else if entropy < 7.0 {
        Color32::from_rgb(255, 180, 100) // Orange - compressed
    } else {
        Color32::from_rgb(255, 100, 100) // Red - encrypted/random
    }
}

/// Map classification to a color.
fn class_to_color(class_id: u8) -> Color32 {
    match class_id {
        0 => Color32::from_rgb(80, 100, 120),   // Zeros - gray-blue
        1 => Color32::from_rgb(100, 180, 100),  // ASCII - green
        2 => Color32::from_rgb(100, 140, 200),  // UTF-8 - blue
        3 => Color32::from_rgb(200, 160, 80),   // Binary - amber
        4 => Color32::from_rgb(200, 80, 80),    // HighEntropy - red
        _ => Color32::GRAY,
    }
}
