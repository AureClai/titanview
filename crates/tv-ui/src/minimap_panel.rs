use egui::{Color32, Rect, Sense, Ui, Vec2, Pos2};
use tv_core::BlockClass;
use crate::state::{AppState, MinimapCache};

/// Right-side minimap showing per-block classification and entropy as a colored vertical bar.
pub struct MinimapPanel;

/// Width of the minimap bar in pixels.
const MINIMAP_WIDTH: f32 = 40.0;

impl MinimapPanel {
    pub fn show(ui: &mut Ui, state: &mut AppState, computing: bool) {
        let entropy = match &state.entropy {
            Some(e) if !e.is_empty() => e,
            _ => {
                // Show progress bar while computing
                if computing {
                    ui.vertical_centered(|ui| {
                        ui.add_space(20.0);
                        ui.spinner();
                        ui.label("Analyzing...");
                    });
                } else {
                    ui.label("No data");
                }
                return;
            }
        };

        let classification = state.classification.as_deref();
        let file_len = state.file_len();
        let num_blocks = entropy.len();
        let has_classification = classification.is_some();

        let available_height = ui.available_height().max(100.0);
        let pixel_rows = (available_height.ceil() as usize).max(1);

        // Check if cache is valid, rebuild if needed
        if !state.minimap_cache.is_valid(pixel_rows, num_blocks, has_classification) {
            Self::rebuild_cache(
                &mut state.minimap_cache,
                entropy,
                classification,
                pixel_rows,
            );
        }

        let (response, painter) = ui.allocate_painter(
            Vec2::new(MINIMAP_WIDTH, available_height),
            Sense::click(),
        );

        let rect = response.rect;
        let row_height = available_height / pixel_rows as f32;

        // Draw cached pixels (fast path - no iteration through blocks)
        for (row, &color) in state.minimap_cache.pixels.iter().enumerate() {
            let y_start = rect.min.y + row as f32 * row_height;
            let y_end = y_start + row_height + 0.5;

            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(rect.min.x, y_start),
                    Pos2::new(rect.max.x, y_end.min(rect.max.y)),
                ),
                0.0,
                color,
            );
        }

        // Viewport indicator
        if file_len > 0 {
            let vp_start_frac = state.viewport.start as f32 / file_len as f32;
            let vp_size_frac = (state.viewport.visible_bytes as f32 / file_len as f32)
                .max(0.005);

            let indicator_y = rect.min.y + vp_start_frac * available_height;
            let indicator_h = vp_size_frac * available_height;

            painter.rect_stroke(
                Rect::from_min_size(
                    Pos2::new(rect.min.x, indicator_y),
                    Vec2::new(MINIMAP_WIDTH, indicator_h),
                ),
                0.0,
                egui::Stroke::new(2.0, Color32::WHITE),
            );
        }

        // Click navigation
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let click_frac = (pos.y - rect.min.y) / available_height;
                let click_frac = click_frac.clamp(0.0, 1.0);
                let target_offset = (click_frac as f64 * file_len as f64) as u64;
                let aligned = (target_offset / 16) * 16;
                state.viewport.start = aligned;
            }
        }

        // Tooltip with classification + entropy
        if response.hovered() {
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                let hover_frac = ((pos.y - rect.min.y) / available_height).clamp(0.0, 1.0);
                let block_idx = (hover_frac * num_blocks as f32) as usize;
                if block_idx < num_blocks {
                    let offset = (hover_frac as f64 * file_len as f64) as u64;
                    let class_label = classification
                        .and_then(|c| c.get(block_idx))
                        .map(|&v| BlockClass::from_u8(v).label())
                        .unwrap_or("N/A");
                    response.on_hover_text(format!(
                        "Block {}: {} | entropy {:.2}\nOffset: 0x{:X}",
                        block_idx, class_label, entropy[block_idx], offset
                    ));
                }
            }
        }
    }

    /// Rebuild the minimap pixel cache.
    /// This is called once when entropy/classification data changes or window resizes.
    /// For a 4GB file, this does 16M+ block iterations ONCE instead of every frame.
    fn rebuild_cache(
        cache: &mut MinimapCache,
        entropy: &[f32],
        classification: Option<&[u8]>,
        pixel_rows: usize,
    ) {
        let num_blocks = entropy.len();

        cache.pixels.clear();
        cache.pixels.reserve(pixel_rows);
        cache.cached_height = pixel_rows;
        cache.cached_block_count = num_blocks;
        cache.cached_has_classification = classification.is_some();

        for row in 0..pixel_rows {
            let frac_start = row as f32 / pixel_rows as f32;
            let frac_end = (row + 1) as f32 / pixel_rows as f32;
            let block_start = (frac_start * num_blocks as f32) as usize;
            let block_end = ((frac_end * num_blocks as f32) as usize).min(num_blocks);

            if block_start >= num_blocks {
                cache.pixels.push(Color32::BLACK);
                continue;
            }

            let max_entropy = entropy[block_start..block_end]
                .iter()
                .copied()
                .fold(0.0f32, f32::max);

            let color = if let Some(classes) = classification {
                let dominant = dominant_block_class(&classes[block_start..block_end.min(classes.len())]);
                classify_entropy_color(dominant, max_entropy)
            } else {
                entropy_to_color(max_entropy)
            };

            cache.pixels.push(color);
        }
    }
}

/// Map an entropy value (0.0 - 8.0) to a color.
/// Low entropy (0.0) = dark blue, high entropy (8.0) = bright red.
pub fn entropy_to_color(entropy: f32) -> Color32 {
    let t = (entropy / 8.0).clamp(0.0, 1.0);

    // Color gradient: dark blue → cyan → green → yellow → red
    let (r, g, b) = if t < 0.25 {
        // Dark blue → cyan
        let s = t / 0.25;
        (0.0, s * 0.8, 0.4 + s * 0.6)
    } else if t < 0.5 {
        // Cyan → green
        let s = (t - 0.25) / 0.25;
        (0.0, 0.8 + s * 0.2, 1.0 - s * 0.6)
    } else if t < 0.75 {
        // Green → yellow
        let s = (t - 0.5) / 0.25;
        (s, 1.0, 0.4 - s * 0.4)
    } else {
        // Yellow → red
        let s = (t - 0.75) / 0.25;
        (1.0, 1.0 - s * 0.8, 0.0)
    };

    Color32::from_rgb(
        (r * 255.0) as u8,
        (g * 255.0) as u8,
        (b * 255.0) as u8,
    )
}

/// Map a block class + entropy to a color.
/// Hue is determined by class, luminosity modulated by entropy.
pub fn classify_entropy_color(class: BlockClass, entropy: f32) -> Color32 {
    // Entropy factor: 0.0 = dim, 8.0 = bright
    let brightness = 0.3 + 0.7 * (entropy / 8.0).clamp(0.0, 1.0);

    let (r, g, b) = match class {
        BlockClass::Zeros => (0.3, 0.35, 0.5),       // grey-blue
        BlockClass::Ascii => (0.2, 0.85, 0.3),       // green
        BlockClass::Utf8 => (0.3, 0.5, 0.95),        // blue
        BlockClass::Binary => (0.9, 0.65, 0.15),     // amber
        BlockClass::HighEntropy => (0.95, 0.15, 0.1), // red
    };

    Color32::from_rgb(
        (r * brightness * 255.0) as u8,
        (g * brightness * 255.0) as u8,
        (b * brightness * 255.0) as u8,
    )
}

/// Return the base color for a BlockClass (used by hex panel offset coloring).
pub fn class_to_subtle_bg(class_id: u8) -> Color32 {
    let class = BlockClass::from_u8(class_id);
    match class {
        BlockClass::Zeros => Color32::from_rgba_premultiplied(40, 50, 70, 60),
        BlockClass::Ascii => Color32::from_rgba_premultiplied(30, 80, 40, 60),
        BlockClass::Utf8 => Color32::from_rgba_premultiplied(40, 60, 100, 60),
        BlockClass::Binary => Color32::from_rgba_premultiplied(90, 65, 20, 60),
        BlockClass::HighEntropy => Color32::from_rgba_premultiplied(100, 25, 15, 60),
    }
}

/// Find the dominant (most frequent) block class in a slice.
/// Used for downsampling classification data to pixel rows.
pub fn dominant_block_class(classes: &[u8]) -> BlockClass {
    if classes.is_empty() {
        return BlockClass::Binary;
    }
    let mut counts = [0u32; 5];
    for &c in classes {
        let idx = (c as usize).min(4);
        counts[idx] += 1;
    }
    let max_idx = counts
        .iter()
        .enumerate()
        .max_by_key(|&(_, &count)| count)
        .map(|(i, _)| i)
        .unwrap_or(3);
    BlockClass::from_u8(max_idx as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entropy_to_color_zero() {
        let c = entropy_to_color(0.0);
        assert_eq!(c.r(), 0);
        assert!(c.b() > 50);
    }

    #[test]
    fn entropy_to_color_max() {
        let c = entropy_to_color(8.0);
        assert_eq!(c.r(), 255);
        assert!(c.g() < 60);
        assert_eq!(c.b(), 0);
    }

    #[test]
    fn entropy_to_color_mid() {
        let c = entropy_to_color(4.0);
        assert!(c.g() > 200);
    }

    #[test]
    fn entropy_to_color_clamps() {
        let _ = entropy_to_color(-1.0);
        let _ = entropy_to_color(10.0);
    }

    #[test]
    fn classify_entropy_color_zeros_dim() {
        let c = classify_entropy_color(BlockClass::Zeros, 0.0);
        // Grey-blue, dim
        assert!(c.r() < 50);
        assert!(c.b() > c.r());
    }

    #[test]
    fn classify_entropy_color_ascii_bright() {
        let c = classify_entropy_color(BlockClass::Ascii, 4.0);
        // Green, moderate brightness
        assert!(c.g() > c.r());
        assert!(c.g() > c.b());
    }

    #[test]
    fn classify_entropy_color_high_entropy() {
        let c = classify_entropy_color(BlockClass::HighEntropy, 8.0);
        // Red, full brightness
        assert!(c.r() > 200);
        assert!(c.g() < 50);
    }

    #[test]
    fn dominant_class_majority_vote() {
        // 3 ASCII, 2 Binary, 1 Zeros
        let classes = [1, 1, 1, 3, 3, 0];
        assert_eq!(dominant_block_class(&classes), BlockClass::Ascii);
    }

    #[test]
    fn dominant_class_empty() {
        assert_eq!(dominant_block_class(&[]), BlockClass::Binary);
    }

    #[test]
    fn dominant_class_single() {
        assert_eq!(dominant_block_class(&[4]), BlockClass::HighEntropy);
    }

    #[test]
    fn click_offset_alignment() {
        let file_len: u64 = 1024;
        let click_frac: f64 = 0.5;
        let target = (click_frac * file_len as f64) as u64;
        let aligned = (target / 16) * 16;
        assert_eq!(aligned, 512);
        assert_eq!(aligned % 16, 0);
    }

    #[test]
    fn click_offset_alignment_uneven() {
        let file_len: u64 = 1000;
        let click_frac: f64 = 0.333;
        let target = (click_frac * file_len as f64) as u64;
        let aligned = (target / 16) * 16;
        assert_eq!(aligned % 16, 0);
        assert!(aligned <= target);
    }
}
