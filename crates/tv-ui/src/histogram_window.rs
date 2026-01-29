//! Byte Histogram visualization window.
//!
//! Shows the distribution of byte values (0-255) in the current file or viewport,
//! useful for identifying encrypted/compressed data patterns.

use egui::{Context, Color32, Pos2, Rect, Stroke, Vec2, FontId, Sense, RichText};
use tv_core::{ByteHistogram, HistogramStats};
use crate::state::AppState;

/// Scope for histogram computation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistogramScope {
    /// Analyze entire file.
    #[default]
    FullFile,
    /// Analyze current viewport only.
    Viewport,
    /// Analyze selection (if any).
    Selection,
}

/// State for the histogram window.
pub struct HistogramState {
    /// Current histogram data.
    pub histogram: Option<ByteHistogram>,
    /// Cached stats.
    pub stats: Option<HistogramStats>,
    /// Analysis scope.
    pub scope: HistogramScope,
    /// Maximum number of bytes to analyze (for large files).
    pub max_bytes: usize,
    /// Hovered byte value.
    pub hovered_byte: Option<u8>,
    /// Display mode.
    pub log_scale: bool,
    /// Show grid lines.
    pub show_grid: bool,
    /// Whether histogram is being computed in background.
    pub computing: bool,
    /// Progress (0.0 - 1.0) for async computation.
    pub progress: f32,
    /// Cached file size/offset for invalidation.
    cached_file_size: u64,
    cached_offset: u64,
}

impl Default for HistogramState {
    fn default() -> Self {
        Self {
            histogram: None,
            stats: None,
            scope: HistogramScope::FullFile,
            max_bytes: 64 * 1024 * 1024, // 64 MB for better coverage
            hovered_byte: None,
            log_scale: false,
            show_grid: true,
            computing: false,
            progress: 0.0,
            cached_file_size: 0,
            cached_offset: u64::MAX,
        }
    }
}

impl HistogramState {
    /// Clear the histogram.
    pub fn clear(&mut self) {
        self.histogram = None;
        self.stats = None;
        self.computing = false;
        self.progress = 0.0;
        self.cached_file_size = 0;
        self.cached_offset = u64::MAX;
    }

    /// Check if histogram needs recomputing.
    pub fn needs_recompute(&self, file_size: u64, offset: u64) -> bool {
        !self.computing
            && self.histogram.is_none()
            || self.cached_file_size != file_size
            || (self.scope == HistogramScope::Viewport && self.cached_offset != offset)
    }

    /// Set the computed histogram result.
    pub fn set_result(&mut self, histogram: ByteHistogram, file_size: u64, offset: u64) {
        self.stats = Some(histogram.stats());
        self.histogram = Some(histogram);
        self.cached_file_size = file_size;
        self.cached_offset = offset;
        self.computing = false;
        self.progress = 1.0;
    }

    /// Get cached file size.
    pub fn cached_file_size(&self) -> u64 {
        self.cached_file_size
    }

    /// Get cached offset.
    pub fn cached_offset(&self) -> u64 {
        self.cached_offset
    }
}

/// Histogram visualization window.
pub struct HistogramWindow;

impl HistogramWindow {
    pub fn show(ctx: &Context, state: &mut AppState, hist_state: &mut HistogramState, visible: &mut bool) {
        if !*visible {
            return;
        }

        egui::Window::new("Byte Histogram")
            .open(visible)
            .default_size([600.0, 400.0])
            .min_size([400.0, 300.0])
            .resizable(true)
            .show(ctx, |ui| {
                Self::show_contents(ui, state, hist_state);
            });
    }

    fn show_contents(ui: &mut egui::Ui, state: &mut AppState, hist_state: &mut HistogramState) {
        if !state.has_file() {
            ui.centered_and_justified(|ui| {
                ui.label("Open a file to view byte histogram.");
            });
            return;
        }

        let file_size = state.file_len();
        let offset = state.viewport.start;

        // Toolbar
        ui.horizontal(|ui| {
            // Scope selector
            ui.label("Scope:");
            egui::ComboBox::from_id_salt("hist_scope")
                .selected_text(match hist_state.scope {
                    HistogramScope::FullFile => "Full File",
                    HistogramScope::Viewport => "Viewport",
                    HistogramScope::Selection => "Selection",
                })
                .show_ui(ui, |ui| {
                    if ui.selectable_value(&mut hist_state.scope, HistogramScope::FullFile, "Full File").changed() {
                        hist_state.clear();
                    }
                    if ui.selectable_value(&mut hist_state.scope, HistogramScope::Viewport, "Viewport").changed() {
                        hist_state.clear();
                    }
                });

            ui.separator();

            ui.checkbox(&mut hist_state.log_scale, "Log scale");
            ui.checkbox(&mut hist_state.show_grid, "Grid");

            ui.separator();

            if ui.button("Refresh").clicked() {
                hist_state.clear();
            }
        });

        // Request histogram computation if needed (will be handled by main.rs)
        if hist_state.needs_recompute(file_size, offset) && !hist_state.computing {
            hist_state.computing = true;
            hist_state.progress = 0.0;
            hist_state.cached_file_size = file_size;
            hist_state.cached_offset = offset;
        }

        // Show progress if computing
        if hist_state.computing {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Computing histogram...");
            });
            if hist_state.progress > 0.0 {
                ui.add(egui::ProgressBar::new(hist_state.progress).animate(true));
            }
        }

        ui.separator();

        // Stats panel
        if let Some(stats) = &hist_state.stats {
            ui.horizontal(|ui| {
                ui.label(format!("Analyzed: {} bytes", format_bytes(stats.total)));
                ui.separator();
                ui.label(format!("Entropy: {:.2} bits", stats.entropy));
                ui.separator();
                ui.label(format!("Unique: {}/256", stats.unique_values));
                ui.separator();

                // Classification
                if let Some(hist) = &hist_state.histogram {
                    let class_text = if hist.looks_encrypted() {
                        RichText::new("Encrypted/Random").color(Color32::from_rgb(255, 100, 100))
                    } else if hist.looks_ascii() {
                        RichText::new("ASCII Text").color(Color32::from_rgb(100, 200, 100))
                    } else if stats.entropy < 4.0 {
                        RichText::new("Low Entropy").color(Color32::from_rgb(100, 150, 255))
                    } else {
                        RichText::new("Binary Data").color(Color32::from_rgb(200, 200, 100))
                    };
                    ui.label(class_text);
                }
            });

            ui.horizontal(|ui| {
                ui.label(format!("Most common: 0x{:02X} ({:.1}%)",
                    stats.most_common,
                    stats.most_common_count as f64 / stats.total as f64 * 100.0
                ));
                ui.separator();
                ui.label(format!("Flatness: {:.1}%", stats.flatness * 100.0));
            });
        }

        ui.separator();

        // Histogram chart - clone to avoid borrow conflict
        if let Some(histogram) = hist_state.histogram.clone() {
            Self::draw_histogram(ui, &histogram, hist_state);
        } else if !hist_state.computing {
            ui.centered_and_justified(|ui| {
                ui.label("Click Refresh to compute histogram.");
            });
        }

        // Tooltip for hovered byte
        if let (Some(byte), Some(hist)) = (hist_state.hovered_byte, &hist_state.histogram) {
            let count = hist.counts[byte as usize];
            let freq = hist.frequency(byte);
            egui::show_tooltip_at_pointer(ui.ctx(), ui.layer_id(), egui::Id::new("hist_tooltip"), |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Byte: 0x{:02X}", byte)).strong());
                    if byte.is_ascii_graphic() || byte == b' ' {
                        ui.label(format!("'{}'", byte as char));
                    }
                });
                ui.label(format!("Count: {}", count));
                ui.label(format!("Frequency: {:.4}%", freq * 100.0));
            });
        }
    }

    fn draw_histogram(ui: &mut egui::Ui, histogram: &ByteHistogram, hist_state: &mut HistogramState) {
        let available = ui.available_size();
        let (response, painter) = ui.allocate_painter(available, Sense::hover());
        let rect = response.rect;

        // Background
        painter.rect_filled(rect, 0.0, Color32::from_rgb(25, 25, 35));

        // Margins
        let margin_left = 50.0;
        let margin_right = 10.0;
        let margin_top = 10.0;
        let margin_bottom = 40.0;

        let chart_rect = Rect::from_min_max(
            Pos2::new(rect.left() + margin_left, rect.top() + margin_top),
            Pos2::new(rect.right() - margin_right, rect.bottom() - margin_bottom),
        );

        // Draw grid
        if hist_state.show_grid {
            Self::draw_grid(&painter, chart_rect, hist_state.log_scale, histogram.max_count());
        }

        // Calculate bar width
        let bar_width = chart_rect.width() / 256.0;
        let max_count = histogram.max_count().max(1) as f64;

        // Draw bars
        hist_state.hovered_byte = None;
        let hover_pos = response.hover_pos();

        for i in 0..=255u8 {
            let count = histogram.counts[i as usize];
            if count == 0 {
                continue;
            }

            let x = chart_rect.left() + i as f32 * bar_width;

            // Calculate height (optionally log scale)
            let height_ratio = if hist_state.log_scale && count > 0 {
                (count as f64).log10() / max_count.log10()
            } else {
                count as f64 / max_count
            };

            // Minimum bar height of 3 pixels for visibility
            let bar_height = (chart_rect.height() * height_ratio as f32).max(3.0);
            let bar_rect = Rect::from_min_max(
                Pos2::new(x, chart_rect.bottom() - bar_height),
                Pos2::new(x + bar_width.max(2.0) - 1.0, chart_rect.bottom()),
            );

            // Color based on byte type
            let color = Self::byte_color(i);

            painter.rect_filled(bar_rect, 0.0, color);

            // Check hover
            if let Some(pos) = hover_pos {
                if bar_rect.contains(pos) || (pos.y >= chart_rect.top() && pos.y <= chart_rect.bottom()
                    && pos.x >= x && pos.x < x + bar_width)
                {
                    hist_state.hovered_byte = Some(i);
                    // Highlight hovered bar
                    painter.rect_stroke(bar_rect.expand(1.0), 0.0, Stroke::new(2.0, Color32::WHITE));
                }
            }
        }

        // Draw axes
        painter.line_segment(
            [Pos2::new(chart_rect.left(), chart_rect.bottom()), Pos2::new(chart_rect.right(), chart_rect.bottom())],
            Stroke::new(1.0, Color32::GRAY),
        );
        painter.line_segment(
            [Pos2::new(chart_rect.left(), chart_rect.top()), Pos2::new(chart_rect.left(), chart_rect.bottom())],
            Stroke::new(1.0, Color32::GRAY),
        );

        // X-axis labels
        for label_val in [0x00, 0x20, 0x40, 0x60, 0x80, 0xA0, 0xC0, 0xE0, 0xFF] {
            let x = chart_rect.left() + label_val as f32 * bar_width;
            painter.text(
                Pos2::new(x, chart_rect.bottom() + 5.0),
                egui::Align2::CENTER_TOP,
                format!("{:02X}", label_val),
                FontId::monospace(9.0),
                Color32::GRAY,
            );
        }

        // X-axis title
        painter.text(
            Pos2::new(chart_rect.center().x, rect.bottom() - 5.0),
            egui::Align2::CENTER_BOTTOM,
            "Byte Value (0x00 - 0xFF)",
            FontId::proportional(10.0),
            Color32::GRAY,
        );

        // Y-axis title (rotated text not supported, use horizontal)
        painter.text(
            Pos2::new(rect.left() + 5.0, chart_rect.center().y),
            egui::Align2::LEFT_CENTER,
            if hist_state.log_scale { "Count\n(log)" } else { "Count" },
            FontId::proportional(10.0),
            Color32::GRAY,
        );

        // Legend
        Self::draw_legend(&painter, rect);
    }

    fn draw_grid(painter: &egui::Painter, chart_rect: Rect, log_scale: bool, max_count: u64) {
        let grid_color = Color32::from_gray(50);

        // Horizontal grid lines
        let y_divisions = if log_scale { 4 } else { 5 };
        for i in 0..=y_divisions {
            let y = chart_rect.top() + chart_rect.height() * i as f32 / y_divisions as f32;
            painter.line_segment(
                [Pos2::new(chart_rect.left(), y), Pos2::new(chart_rect.right(), y)],
                Stroke::new(1.0, grid_color),
            );

            // Y-axis labels
            let value = if log_scale {
                let log_max = (max_count as f64).log10();
                let log_val = log_max * (y_divisions - i) as f64 / y_divisions as f64;
                10.0_f64.powf(log_val) as u64
            } else {
                max_count * (y_divisions - i) as u64 / y_divisions as u64
            };

            painter.text(
                Pos2::new(chart_rect.left() - 5.0, y),
                egui::Align2::RIGHT_CENTER,
                format_count(value),
                FontId::monospace(9.0),
                Color32::GRAY,
            );
        }

        // Vertical grid lines at key byte ranges
        for x_val in [0x20, 0x7F, 0x80] {
            let x = chart_rect.left() + x_val as f32 * chart_rect.width() / 256.0;
            painter.line_segment(
                [Pos2::new(x, chart_rect.top()), Pos2::new(x, chart_rect.bottom())],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(100, 100, 100, 100)),
            );
        }
    }

    fn draw_legend(painter: &egui::Painter, rect: Rect) {
        let legend_y = rect.top() + 5.0;
        let mut x = rect.right() - 200.0;
        let box_size = 10.0;
        let spacing = 50.0;

        let items = [
            (Color32::from_rgb(80, 100, 140), "Null/Ctrl"),
            (Color32::from_rgb(100, 220, 100), "ASCII"),
            (Color32::from_rgb(100, 160, 255), "High"),
        ];

        for (color, label) in items {
            painter.rect_filled(
                Rect::from_min_size(Pos2::new(x, legend_y), Vec2::new(box_size, box_size)),
                0.0,
                color,
            );
            painter.text(
                Pos2::new(x + box_size + 3.0, legend_y),
                egui::Align2::LEFT_TOP,
                label,
                FontId::proportional(9.0),
                Color32::GRAY,
            );
            x += spacing;
        }
    }

    fn byte_color(byte: u8) -> Color32 {
        match byte {
            0x00 => Color32::from_rgb(80, 100, 140),            // Null - blue-gray (more visible)
            0x01..=0x1F => Color32::from_rgb(140, 100, 140),    // Control chars - purple
            0x20..=0x7E => Color32::from_rgb(100, 220, 100),    // Printable ASCII - green
            0x7F => Color32::from_rgb(140, 100, 140),           // DEL - purple
            0x80..=0xFF => Color32::from_rgb(100, 160, 255),    // High bytes - blue
        }
    }
}

/// Format byte count with K/M/G suffix.
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Format count with K/M suffix.
fn format_count(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.0}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.0}K", count as f64 / 1_000.0)
    } else {
        format!("{}", count)
    }
}
