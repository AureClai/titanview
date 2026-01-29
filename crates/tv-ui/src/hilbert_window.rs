use egui::{Context, Color32, ColorImage, TextureHandle, TextureOptions, Vec2};
use crate::state::AppState;

/// Hilbert curve visualization mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HilbertMode {
    #[default]
    Entropy,
    Classification,
    ByteValue,
    /// Bit density - each pixel is a single bit (0 = dark, 1 = bright)
    BitDensity,
}

impl HilbertMode {
    pub fn label(&self) -> &'static str {
        match self {
            HilbertMode::Entropy => "Entropy",
            HilbertMode::Classification => "Classification",
            HilbertMode::ByteValue => "Byte Value",
            HilbertMode::BitDensity => "Bit Density",
        }
    }

    pub fn as_u32(&self) -> u32 {
        match self {
            HilbertMode::Entropy => 0,
            HilbertMode::Classification => 1,
            HilbertMode::ByteValue => 2,
            HilbertMode::BitDensity => 3,
        }
    }
}

/// State for the Hilbert visualization window.
pub struct HilbertState {
    /// Current visualization mode.
    pub mode: HilbertMode,
    /// Texture size (power of 2).
    pub texture_size: u32,
    /// Cached texture handle.
    pub texture: Option<TextureHandle>,
    /// File size when texture was computed (for invalidation).
    cached_file_size: u64,
    /// Mode when texture was computed (for invalidation).
    cached_mode: HilbertMode,
    /// Whether computation is in progress.
    pub computing: bool,
    /// Pending pixel data from background computation.
    pub pending_pixels: Option<Vec<u32>>,
    /// Last computation time in ms.
    pub compute_time_ms: Option<f64>,
}

impl Default for HilbertState {
    fn default() -> Self {
        Self {
            mode: HilbertMode::Entropy,
            texture_size: 512,
            texture: None,
            cached_file_size: 0,
            cached_mode: HilbertMode::Entropy,
            computing: false,
            pending_pixels: None,
            compute_time_ms: None,
        }
    }
}

impl HilbertState {
    /// Check if the cached texture is still valid.
    pub fn is_valid(&self, file_size: u64) -> bool {
        self.texture.is_some()
            && self.cached_file_size == file_size
            && self.cached_mode == self.mode
    }

    /// Mark cache as invalid (e.g., when file changes).
    pub fn invalidate(&mut self) {
        self.texture = None;
        self.cached_file_size = 0;
    }

    /// Update cache with new texture.
    pub fn update_texture(&mut self, ctx: &Context, pixels: Vec<u32>, file_size: u64) {
        let size = self.texture_size as usize;

        // Convert u32 RGBA to Color32 array
        let colors: Vec<Color32> = pixels
            .iter()
            .map(|&p| {
                let r = (p & 0xFF) as u8;
                let g = ((p >> 8) & 0xFF) as u8;
                let b = ((p >> 16) & 0xFF) as u8;
                let a = ((p >> 24) & 0xFF) as u8;
                Color32::from_rgba_unmultiplied(r, g, b, a)
            })
            .collect();

        let image = ColorImage {
            size: [size, size],
            pixels: colors,
        };

        let texture = ctx.load_texture(
            "hilbert_texture",
            image,
            TextureOptions::NEAREST,
        );

        self.texture = Some(texture);
        self.cached_file_size = file_size;
        self.cached_mode = self.mode;
    }
}

/// Floating window displaying Hilbert curve visualization.
pub struct HilbertWindow;

impl HilbertWindow {
    pub fn show(ctx: &Context, state: &mut AppState, hilbert: &mut HilbertState, visible: &mut bool) {
        if !*visible {
            return;
        }

        egui::Window::new("Hilbert Curve")
            .open(visible)
            .default_size([550.0, 600.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                Self::show_contents(ui, ctx, state, hilbert);
            });
    }

    fn show_contents(ui: &mut egui::Ui, ctx: &Context, state: &mut AppState, hilbert: &mut HilbertState) {
        if !state.has_file() {
            ui.label("Open a file to visualize.");
            return;
        }

        let file_size = state.file_len();

        // Controls
        ui.horizontal(|ui| {
            ui.label("Mode:");
            egui::ComboBox::from_id_salt("hilbert_mode")
                .selected_text(hilbert.mode.label())
                .show_ui(ui, |ui| {
                    if ui.selectable_value(&mut hilbert.mode, HilbertMode::Entropy, "Entropy").changed() {
                        hilbert.invalidate();
                    }
                    if ui.selectable_value(&mut hilbert.mode, HilbertMode::Classification, "Classification").changed() {
                        hilbert.invalidate();
                    }
                    if ui.selectable_value(&mut hilbert.mode, HilbertMode::ByteValue, "Byte Value").changed() {
                        hilbert.invalidate();
                    }
                    if ui.selectable_value(&mut hilbert.mode, HilbertMode::BitDensity, "Bit Density").changed() {
                        hilbert.invalidate();
                    }
                });

            ui.separator();

            ui.label("Size:");
            egui::ComboBox::from_id_salt("hilbert_size")
                .selected_text(format!("{}x{}", hilbert.texture_size, hilbert.texture_size))
                .show_ui(ui, |ui| {
                    for size in [256, 512, 1024] {
                        if ui.selectable_value(&mut hilbert.texture_size, size, format!("{}x{}", size, size)).changed() {
                            hilbert.invalidate();
                        }
                    }
                });
        });

        // Check if we need to compute
        let needs_compute = !hilbert.is_valid(file_size) && !hilbert.computing;

        if needs_compute {
            ui.horizontal(|ui| {
                if ui.button("Generate").clicked() {
                    hilbert.computing = true;
                }
                ui.weak("Click to compute Hilbert visualization");
            });
        }

        if hilbert.computing && hilbert.pending_pixels.is_none() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Computing...");
            });
        }

        // Handle pending pixels
        if let Some(pixels) = hilbert.pending_pixels.take() {
            hilbert.update_texture(ctx, pixels, file_size);
            hilbert.computing = false;
        }

        // Stats
        if let Some(ms) = hilbert.compute_time_ms {
            ui.weak(format!("Computed in {:.1} ms", ms));
        }

        ui.separator();

        // Display texture
        if let Some(ref texture) = hilbert.texture {
            let available_size = ui.available_size();
            let tex_size = hilbert.texture_size as f32;

            // Scale to fit while maintaining aspect ratio
            let scale = (available_size.x.min(available_size.y) / tex_size).min(1.5);
            let display_size = Vec2::splat(tex_size * scale);

            let response = ui.add(
                egui::Image::new(texture)
                    .fit_to_exact_size(display_size)
                    .sense(egui::Sense::click())
            );

            // Handle clicks - navigate to the corresponding offset
            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let rect = response.rect;
                    let rel_x = (pos.x - rect.left()) / rect.width();
                    let rel_y = (pos.y - rect.top()) / rect.height();

                    let x = (rel_x * tex_size) as u32;
                    let y = (rel_y * tex_size) as u32;

                    // Convert (x, y) to Hilbert index, then to file offset
                    let hilbert_index = xy2d(hilbert.texture_size, x, y);
                    let total_pixels = (hilbert.texture_size * hilbert.texture_size) as u64;
                    let bytes_per_pixel = (file_size / total_pixels).max(1);
                    let offset = (hilbert_index as u64) * bytes_per_pixel;

                    if offset < file_size {
                        state.viewport.start = (offset / 16) * 16;
                    }
                }
            }

            // Show tooltip with offset info on hover
            if response.hovered() {
                if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                    let rect = response.rect;
                    if rect.contains(pos) {
                        let rel_x = (pos.x - rect.left()) / rect.width();
                        let rel_y = (pos.y - rect.top()) / rect.height();

                        let x = (rel_x * tex_size) as u32;
                        let y = (rel_y * tex_size) as u32;

                        let hilbert_index = xy2d(hilbert.texture_size, x, y);
                        let total_pixels = (hilbert.texture_size * hilbert.texture_size) as u64;
                        let bytes_per_pixel = (file_size / total_pixels).max(1);
                        let offset = (hilbert_index as u64) * bytes_per_pixel;

                        response.on_hover_text(format!(
                            "Offset: 0x{:X}\nPixel: ({}, {})\nHilbert index: {}",
                            offset, x, y, hilbert_index
                        ));
                    }
                }
            }
        } else if !hilbert.computing {
            // Show placeholder
            let size = Vec2::splat(hilbert.texture_size as f32 * 0.8);
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            ui.painter().rect_filled(rect, 4.0, Color32::from_gray(30));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Click 'Generate' to compute",
                egui::FontId::default(),
                Color32::GRAY,
            );
        }

        // Legend
        ui.add_space(8.0);
        Self::show_legend(ui, hilbert.mode);
    }

    fn show_legend(ui: &mut egui::Ui, mode: HilbertMode) {
        ui.collapsing("Legend", |ui| {
            match mode {
                HilbertMode::Entropy => {
                    ui.horizontal(|ui| {
                        Self::color_box(ui, Color32::from_rgb(0, 128, 90));
                        ui.label("Low entropy (structured)");
                    });
                    ui.horizontal(|ui| {
                        Self::color_box(ui, Color32::from_rgb(77, 204, 77));
                        ui.label("Medium-low");
                    });
                    ui.horizontal(|ui| {
                        Self::color_box(ui, Color32::from_rgb(204, 153, 77));
                        ui.label("Medium-high");
                    });
                    ui.horizontal(|ui| {
                        Self::color_box(ui, Color32::from_rgb(255, 51, 26));
                        ui.label("High entropy (random/encrypted)");
                    });
                }
                HilbertMode::Classification => {
                    ui.horizontal(|ui| {
                        Self::color_box(ui, Color32::from_rgb(51, 64, 89));
                        ui.label("Zeros");
                    });
                    ui.horizontal(|ui| {
                        Self::color_box(ui, Color32::from_rgb(77, 179, 77));
                        ui.label("ASCII");
                    });
                    ui.horizontal(|ui| {
                        Self::color_box(ui, Color32::from_rgb(77, 128, 204));
                        ui.label("UTF-8");
                    });
                    ui.horizontal(|ui| {
                        Self::color_box(ui, Color32::from_rgb(204, 153, 77));
                        ui.label("Binary");
                    });
                    ui.horizontal(|ui| {
                        Self::color_box(ui, Color32::from_rgb(204, 51, 51));
                        ui.label("High Entropy");
                    });
                }
                HilbertMode::ByteValue => {
                    ui.horizontal(|ui| {
                        Self::color_box(ui, Color32::from_rgb(13, 13, 26));
                        ui.label("0x00 (null)");
                    });
                    ui.horizontal(|ui| {
                        Self::color_box(ui, Color32::from_rgb(102, 128, 102));
                        ui.label("Printable ASCII (green tint)");
                    });
                    ui.horizontal(|ui| {
                        Self::color_box(ui, Color32::from_rgb(128, 128, 128));
                        ui.label("Other bytes (grayscale)");
                    });
                    ui.horizontal(|ui| {
                        Self::color_box(ui, Color32::from_rgb(255, 255, 230));
                        ui.label("0xFF");
                    });
                }
                HilbertMode::BitDensity => {
                    ui.label("Each pixel = 1 bit");
                    ui.horizontal(|ui| {
                        Self::color_box(ui, Color32::from_rgb(10, 10, 20));
                        ui.label("0 bit");
                    });
                    ui.horizontal(|ui| {
                        Self::color_box(ui, Color32::from_rgb(0, 255, 128));
                        ui.label("1 bit");
                    });
                    ui.add_space(4.0);
                    ui.label("Reveals bit-level patterns:");
                    ui.weak("- Synchronization markers");
                    ui.weak("- Stream cipher patterns");
                    ui.weak("- Padding structures");
                }
            }
        });
    }

    fn color_box(ui: &mut egui::Ui, color: Color32) {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 2.0, color);
    }
}

/// Convert (x, y) coordinates to Hilbert curve index.
/// n = size of grid (power of 2)
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
