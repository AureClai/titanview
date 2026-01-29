//! Control Flow Graph visualization window.
//!
//! Renders the CFG with pan/zoom support, showing basic blocks
//! as boxes connected by arrows for control flow edges.

use egui::{Context, Color32, Pos2, Rect, Stroke, Vec2, FontId, Sense};
use tv_core::{ControlFlowGraph, BasicBlock, EdgeType};

/// State for the CFG visualization window.
#[derive(Default)]
pub struct CfgState {
    /// The computed CFG (if any).
    pub cfg: Option<ControlFlowGraph>,
    /// Pan offset for the view.
    pub pan: Vec2,
    /// Zoom level (1.0 = 100%).
    pub zoom: f32,
    /// Currently hovered block address.
    pub hovered_block: Option<u64>,
    /// Currently selected block address.
    pub selected_block: Option<u64>,
    /// Whether CFG is being computed.
    pub computing: bool,
}

impl CfgState {
    pub fn new() -> Self {
        Self {
            cfg: None,
            pan: Vec2::ZERO,
            zoom: 1.0,
            hovered_block: None,
            selected_block: None,
            computing: false,
        }
    }

    /// Reset the view to center on the graph.
    pub fn reset_view(&mut self) {
        self.pan = Vec2::ZERO;
        self.zoom = 1.0;
    }

    /// Clear the CFG.
    pub fn clear(&mut self) {
        self.cfg = None;
        self.hovered_block = None;
        self.selected_block = None;
        self.computing = false;
    }
}

/// CFG visualization window.
pub struct CfgWindow;

impl CfgWindow {
    pub fn show(ctx: &Context, state: &mut CfgState, visible: &mut bool) {
        if !*visible {
            return;
        }

        egui::Window::new("Control Flow Graph")
            .open(visible)
            .default_size([800.0, 600.0])
            .min_size([400.0, 300.0])
            .resizable(true)
            .show(ctx, |ui| {
                Self::show_contents(ui, state);
            });
    }

    fn show_contents(ui: &mut egui::Ui, state: &mut CfgState) {
        // Toolbar
        ui.horizontal(|ui| {
            if ui.button("Reset View").clicked() {
                state.reset_view();
            }

            ui.separator();

            ui.label("Zoom:");
            if ui.button("-").clicked() {
                state.zoom = (state.zoom - 0.1).max(0.2);
            }
            ui.label(format!("{:.0}%", state.zoom * 100.0));
            if ui.button("+").clicked() {
                state.zoom = (state.zoom + 0.1).min(3.0);
            }

            ui.separator();

            if let Some(cfg) = &state.cfg {
                ui.label(format!("{} blocks, {} edges", cfg.blocks.len(), cfg.edges.len()));
            }

            if state.computing {
                ui.separator();
                ui.spinner();
                ui.label("Building CFG...");
            }
        });

        ui.separator();

        // Check if we have a CFG to render
        let cfg = match &state.cfg {
            Some(cfg) => cfg,
            None => {
                ui.centered_and_justified(|ui| {
                    ui.label("No CFG to display.\nSelect a function in the disassembler and click 'Show CFG'.");
                });
                return;
            }
        };

        // Debug info if no edges
        if cfg.edges.is_empty() && !cfg.blocks.is_empty() {
            ui.colored_label(Color32::YELLOW, "Debug: No edges found. Analyzing why...");
            ui.horizontal(|ui| {
                // Count control flow instructions
                let mut jump_count = 0;
                let mut call_count = 0;
                let mut ret_count = 0;
                let mut targets_in_range = 0;
                let mut targets_out_of_range = 0;
                let mut indirect_jumps = 0;

                for block in cfg.blocks.values() {
                    for instr in &block.instructions {
                        if instr.is_jump() {
                            jump_count += 1;
                            if let Some(target) = instr.target_address() {
                                if cfg.blocks.contains_key(&target) {
                                    targets_in_range += 1;
                                } else {
                                    targets_out_of_range += 1;
                                }
                            } else {
                                indirect_jumps += 1;
                            }
                        }
                        if instr.is_call() { call_count += 1; }
                        if instr.is_return() { ret_count += 1; }
                    }
                }

                ui.label(format!("Jumps: {}, Calls: {}, Rets: {}", jump_count, call_count, ret_count));
                ui.separator();
                ui.label(format!("Targets in range: {}, out: {}, indirect: {}",
                    targets_in_range, targets_out_of_range, indirect_jumps));
            });
            ui.separator();
        }

        // Main canvas area
        let available = ui.available_size();
        let (response, painter) = ui.allocate_painter(available, Sense::click_and_drag());
        let canvas_rect = response.rect;

        // Handle pan
        if response.dragged() {
            state.pan += response.drag_delta();
        }

        // Handle zoom with scroll wheel
        if response.hovered() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll != 0.0 {
                let zoom_delta = scroll * 0.001;
                state.zoom = (state.zoom + zoom_delta).clamp(0.2, 3.0);
            }
        }

        // Calculate transform
        let center = canvas_rect.center();
        let transform = |pos: Pos2| -> Pos2 {
            let scaled = Pos2::new(pos.x * state.zoom, pos.y * state.zoom);
            center + state.pan + scaled.to_vec2()
        };

        // Draw background grid
        Self::draw_grid(&painter, canvas_rect, state.zoom, state.pan);

        // Draw edges first (behind blocks)
        for edge in &cfg.edges {
            if let (Some(from_block), Some(to_block)) = (cfg.blocks.get(&edge.from), cfg.blocks.get(&edge.to)) {
                Self::draw_edge(&painter, from_block, to_block, edge.edge_type, &transform, state.zoom);
            }
        }

        // Draw blocks
        let mut new_hovered = None;
        for block in cfg.blocks.values() {
            let block_rect = Self::draw_block(&painter, block, &transform, state, canvas_rect);

            // Check hover
            if let Some(rect) = block_rect {
                if rect.contains(response.hover_pos().unwrap_or(Pos2::ZERO)) {
                    new_hovered = Some(block.start_addr);
                }
            }
        }
        state.hovered_block = new_hovered;

        // Handle click to select block
        if response.clicked() {
            state.selected_block = state.hovered_block;
        }

        // Draw selected block info
        if let Some(addr) = state.selected_block {
            if let Some(block) = cfg.blocks.get(&addr) {
                Self::draw_block_info(ui, block, canvas_rect);
            }
        }
    }

    fn draw_grid(painter: &egui::Painter, rect: Rect, zoom: f32, pan: Vec2) {
        let grid_size = 50.0 * zoom;
        let grid_color = Color32::from_gray(40);

        let center = rect.center() + pan;

        // Vertical lines
        let start_x = ((rect.left() - center.x) / grid_size).floor() as i32;
        let end_x = ((rect.right() - center.x) / grid_size).ceil() as i32;
        for i in start_x..=end_x {
            let x = center.x + i as f32 * grid_size;
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0, grid_color),
            );
        }

        // Horizontal lines
        let start_y = ((rect.top() - center.y) / grid_size).floor() as i32;
        let end_y = ((rect.bottom() - center.y) / grid_size).ceil() as i32;
        for i in start_y..=end_y {
            let y = center.y + i as f32 * grid_size;
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(1.0, grid_color),
            );
        }
    }

    fn draw_block(
        painter: &egui::Painter,
        block: &BasicBlock,
        transform: &impl Fn(Pos2) -> Pos2,
        state: &CfgState,
        clip_rect: Rect,
    ) -> Option<Rect> {
        let top_left = transform(Pos2::new(block.layout_x, block.layout_y));
        let width = block.render_width() * state.zoom;
        let height = block.render_height() * state.zoom;
        let rect = Rect::from_min_size(top_left, Vec2::new(width, height));

        // Skip if outside clip rect
        if !rect.intersects(clip_rect) {
            return None;
        }

        // Determine colors based on state
        let is_hovered = state.hovered_block == Some(block.start_addr);
        let is_selected = state.selected_block == Some(block.start_addr);
        let is_entry = state.cfg.as_ref().is_some_and(|c| c.entry == block.start_addr);
        let is_exit = block.ends_with_return();

        let bg_color = if is_selected {
            Color32::from_rgb(60, 60, 100)
        } else if is_hovered {
            Color32::from_rgb(50, 50, 70)
        } else if is_entry {
            Color32::from_rgb(40, 60, 40)
        } else if is_exit {
            Color32::from_rgb(60, 40, 40)
        } else {
            Color32::from_rgb(35, 35, 45)
        };

        let border_color = if is_selected {
            Color32::from_rgb(100, 150, 255)
        } else if is_hovered {
            Color32::from_rgb(150, 150, 200)
        } else if is_entry {
            Color32::from_rgb(100, 200, 100)
        } else if is_exit {
            Color32::from_rgb(200, 100, 100)
        } else {
            Color32::from_rgb(80, 80, 100)
        };

        // Draw block background
        painter.rect_filled(rect, 4.0, bg_color);
        painter.rect_stroke(rect, 4.0, Stroke::new(2.0, border_color));

        // Draw header (address)
        let header_rect = Rect::from_min_size(
            top_left,
            Vec2::new(width, 18.0 * state.zoom),
        );
        painter.rect_filled(header_rect, 4.0, Color32::from_rgb(50, 50, 70));

        let header_text = format!("{:08X}", block.start_addr);
        let font_size = 12.0 * state.zoom;
        painter.text(
            header_rect.center(),
            egui::Align2::CENTER_CENTER,
            header_text,
            FontId::monospace(font_size),
            Color32::from_rgb(150, 200, 255),
        );

        // Draw instructions (only if zoomed in enough)
        if state.zoom >= 0.5 {
            let instr_font_size = 10.0 * state.zoom;
            let line_height = 12.0 * state.zoom;
            let mut y = top_left.y + 20.0 * state.zoom;

            for instr in &block.instructions {
                if y > rect.bottom() - 5.0 {
                    break;
                }

                let instr_text = format!("{} {}", instr.mnemonic, instr.operands);
                let color = Self::instruction_color(&instr.mnemonic);

                painter.text(
                    Pos2::new(top_left.x + 5.0 * state.zoom, y),
                    egui::Align2::LEFT_TOP,
                    &instr_text,
                    FontId::monospace(instr_font_size),
                    color,
                );

                y += line_height;
            }
        }

        Some(rect)
    }

    fn instruction_color(mnemonic: &str) -> Color32 {
        let m = mnemonic.to_lowercase();
        if m == "jmp" || m.starts_with("j") {
            Color32::from_rgb(255, 200, 100) // Yellow for jumps
        } else if m == "call" {
            Color32::from_rgb(100, 200, 255) // Cyan for calls
        } else if m == "ret" || m == "retn" {
            Color32::from_rgb(255, 100, 100) // Red for returns
        } else if m == "push" || m == "pop" {
            Color32::from_rgb(200, 150, 255) // Purple for stack ops
        } else if m == "mov" || m == "lea" {
            Color32::from_rgb(150, 255, 150) // Green for data movement
        } else if m == "cmp" || m == "test" {
            Color32::from_rgb(255, 150, 150) // Pink for comparisons
        } else {
            Color32::from_rgb(200, 200, 200) // White for others
        }
    }

    fn draw_edge(
        painter: &egui::Painter,
        from: &BasicBlock,
        to: &BasicBlock,
        edge_type: EdgeType,
        transform: &impl Fn(Pos2) -> Pos2,
        zoom: f32,
    ) {
        let from_center_x = from.layout_x + from.render_width() / 2.0;
        let from_bottom = from.layout_y + from.render_height();
        let to_center_x = to.layout_x + to.render_width() / 2.0;
        let to_top = to.layout_y;

        let start = transform(Pos2::new(from_center_x, from_bottom));
        let end = transform(Pos2::new(to_center_x, to_top));

        let (color, width) = match edge_type {
            EdgeType::Unconditional => (Color32::from_rgb(150, 150, 150), 2.0),
            EdgeType::ConditionalTrue => (Color32::from_rgb(100, 200, 100), 2.0),
            EdgeType::ConditionalFalse => (Color32::from_rgb(200, 100, 100), 2.0),
            EdgeType::Call => (Color32::from_rgb(100, 150, 255), 1.5),
        };

        // Handle back edges (going up)
        if to.layout_y <= from.layout_y {
            // Draw as curved line going around
            let offset = 30.0 * zoom;
            let side = if to_center_x < from_center_x { -1.0 } else { 1.0 };

            let mid_x = (from_center_x.min(to_center_x) - 50.0) * side;
            let ctrl1 = transform(Pos2::new(from_center_x + offset * side, from_bottom + offset));
            let ctrl2 = transform(Pos2::new(mid_x, (from_bottom + to_top) / 2.0));
            let ctrl3 = transform(Pos2::new(to_center_x + offset * side, to_top - offset));

            // Draw as polyline for back edge
            let points = [start, ctrl1, ctrl2, ctrl3, end];
            for i in 0..points.len() - 1 {
                painter.line_segment([points[i], points[i + 1]], Stroke::new(width * zoom, color));
            }
        } else {
            // Normal edge: bezier curve
            let mid_y = (start.y + end.y) / 2.0;
            let ctrl1 = Pos2::new(start.x, mid_y);
            let ctrl2 = Pos2::new(end.x, mid_y);

            // Draw as bezier
            let bezier = egui::epaint::CubicBezierShape::from_points_stroke(
                [start, ctrl1, ctrl2, end],
                false,
                Color32::TRANSPARENT,
                Stroke::new(width * zoom, color),
            );
            painter.add(bezier);
        }

        // Draw arrowhead
        Self::draw_arrowhead(painter, end, to.layout_y < from.layout_y, color, zoom);
    }

    fn draw_arrowhead(painter: &egui::Painter, tip: Pos2, pointing_up: bool, color: Color32, zoom: f32) {
        let size = 8.0 * zoom;
        let dir = if pointing_up { -1.0 } else { 1.0 };

        let left = Pos2::new(tip.x - size / 2.0, tip.y - size * dir);
        let right = Pos2::new(tip.x + size / 2.0, tip.y - size * dir);

        painter.add(egui::Shape::convex_polygon(
            vec![tip, left, right],
            color,
            Stroke::NONE,
        ));
    }

    fn draw_block_info(ui: &mut egui::Ui, block: &BasicBlock, canvas_rect: Rect) {
        // Draw info panel in corner
        let panel_rect = Rect::from_min_size(
            Pos2::new(canvas_rect.right() - 250.0, canvas_rect.top() + 5.0),
            Vec2::new(240.0, 150.0),
        );

        ui.painter().rect_filled(panel_rect, 4.0, Color32::from_rgba_unmultiplied(30, 30, 40, 230));
        ui.painter().rect_stroke(panel_rect, 4.0, Stroke::new(1.0, Color32::from_rgb(80, 80, 100)));

        // Draw text manually
        let mut y = panel_rect.top() + 10.0;
        let x = panel_rect.left() + 10.0;
        let font = FontId::monospace(11.0);

        ui.painter().text(
            Pos2::new(x, y),
            egui::Align2::LEFT_TOP,
            format!("Block: {:08X}", block.start_addr),
            font.clone(),
            Color32::from_rgb(150, 200, 255),
        );
        y += 16.0;

        ui.painter().text(
            Pos2::new(x, y),
            egui::Align2::LEFT_TOP,
            format!("Instructions: {}", block.instructions.len()),
            font.clone(),
            Color32::from_rgb(200, 200, 200),
        );
        y += 16.0;

        ui.painter().text(
            Pos2::new(x, y),
            egui::Align2::LEFT_TOP,
            format!("Successors: {}", block.successors.len()),
            font.clone(),
            Color32::from_rgb(200, 200, 200),
        );
        y += 16.0;

        ui.painter().text(
            Pos2::new(x, y),
            egui::Align2::LEFT_TOP,
            format!("Predecessors: {}", block.predecessors.len()),
            font.clone(),
            Color32::from_rgb(200, 200, 200),
        );
        y += 20.0;

        // Show first few instructions
        ui.painter().text(
            Pos2::new(x, y),
            egui::Align2::LEFT_TOP,
            "Instructions:",
            font.clone(),
            Color32::from_rgb(150, 150, 150),
        );
        y += 14.0;

        for instr in block.instructions.iter().take(4) {
            ui.painter().text(
                Pos2::new(x + 5.0, y),
                egui::Align2::LEFT_TOP,
                format!("{} {}", instr.mnemonic, instr.operands),
                FontId::monospace(10.0),
                Self::instruction_color(&instr.mnemonic),
            );
            y += 12.0;
        }

        if block.instructions.len() > 4 {
            ui.painter().text(
                Pos2::new(x + 5.0, y),
                egui::Align2::LEFT_TOP,
                format!("... +{} more", block.instructions.len() - 4),
                FontId::monospace(10.0),
                Color32::from_rgb(120, 120, 120),
            );
        }
    }
}
