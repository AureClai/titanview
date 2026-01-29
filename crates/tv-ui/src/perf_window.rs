use egui::{Color32, Ui, Vec2};
use std::collections::VecDeque;
use std::time::Instant;

/// Number of samples to keep in history for graphs.
const HISTORY_SIZE: usize = 120;

/// Performance metrics state.
pub struct PerfState {
    /// Whether the performance window is visible.
    pub visible: bool,
    /// Frame time history (in ms).
    frame_times: VecDeque<f32>,
    /// Memory usage history (in MB).
    memory_usage: VecDeque<f32>,
    /// Last frame timestamp.
    last_frame: Instant,
    /// Frame count for FPS calculation.
    frame_count: u32,
    /// Last FPS update time.
    last_fps_update: Instant,
    /// Current FPS value.
    current_fps: f32,
    /// Peak frame time (ms).
    peak_frame_time: f32,
    /// Total frames rendered.
    total_frames: u64,
}

impl Default for PerfState {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            visible: false,
            frame_times: VecDeque::with_capacity(HISTORY_SIZE),
            memory_usage: VecDeque::with_capacity(HISTORY_SIZE),
            last_frame: now,
            frame_count: 0,
            last_fps_update: now,
            current_fps: 0.0,
            peak_frame_time: 0.0,
            total_frames: 0,
        }
    }
}

impl PerfState {
    /// Call this at the start of each frame to update metrics.
    pub fn begin_frame(&mut self) {
        let now = Instant::now();
        let frame_time = now.duration_since(self.last_frame).as_secs_f32() * 1000.0;
        self.last_frame = now;

        // Record frame time
        if self.frame_times.len() >= HISTORY_SIZE {
            self.frame_times.pop_front();
        }
        self.frame_times.push_back(frame_time);

        // Track peak
        if frame_time > self.peak_frame_time {
            self.peak_frame_time = frame_time;
        }

        // Update FPS every second
        self.frame_count += 1;
        self.total_frames += 1;
        let elapsed = now.duration_since(self.last_fps_update).as_secs_f32();
        if elapsed >= 1.0 {
            self.current_fps = self.frame_count as f32 / elapsed;
            self.frame_count = 0;
            self.last_fps_update = now;

            // Sample memory usage once per second
            self.sample_memory();
        }
    }

    /// Sample current memory usage.
    fn sample_memory(&mut self) {
        let mem_mb = get_memory_usage_mb();
        if self.memory_usage.len() >= HISTORY_SIZE {
            self.memory_usage.pop_front();
        }
        self.memory_usage.push_back(mem_mb);
    }

    /// Reset peak frame time.
    pub fn reset_peak(&mut self) {
        self.peak_frame_time = 0.0;
    }

    /// Get average frame time over history.
    fn avg_frame_time(&self) -> f32 {
        if self.frame_times.is_empty() {
            return 0.0;
        }
        self.frame_times.iter().sum::<f32>() / self.frame_times.len() as f32
    }

    /// Get min frame time over history.
    fn min_frame_time(&self) -> f32 {
        self.frame_times.iter().copied().fold(f32::MAX, f32::min)
    }

    /// Get max frame time over history.
    fn max_frame_time(&self) -> f32 {
        self.frame_times.iter().copied().fold(0.0, f32::max)
    }

    /// Get current FPS (public for menu bar display).
    pub fn current_fps(&self) -> f32 {
        self.current_fps
    }
}

/// Get current process memory usage in MB.
#[cfg(windows)]
fn get_memory_usage_mb() -> f32 {
    use std::mem::MaybeUninit;

    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "psapi")]
    extern "system" {
        fn GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            pmc: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
    }

    unsafe {
        let mut pmc = MaybeUninit::<ProcessMemoryCounters>::zeroed();
        (*pmc.as_mut_ptr()).cb = std::mem::size_of::<ProcessMemoryCounters>() as u32;

        if GetProcessMemoryInfo(
            GetCurrentProcess(),
            pmc.as_mut_ptr(),
            std::mem::size_of::<ProcessMemoryCounters>() as u32,
        ) != 0 {
            let pmc = pmc.assume_init();
            return pmc.working_set_size as f32 / (1024.0 * 1024.0);
        }
    }
    0.0
}

#[cfg(not(windows))]
fn get_memory_usage_mb() -> f32 {
    // Fallback for non-Windows: try to read /proc/self/statm
    if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
        if let Some(rss) = statm.split_whitespace().nth(1) {
            if let Ok(pages) = rss.parse::<u64>() {
                // Assume 4KB pages
                return (pages * 4096) as f32 / (1024.0 * 1024.0);
            }
        }
    }
    0.0
}

/// Floating performance window.
pub struct PerfWindow;

impl PerfWindow {
    pub fn show(ctx: &egui::Context, state: &mut PerfState) {
        if !state.visible {
            return;
        }

        egui::Window::new("Performance")
            .default_size([320.0, 400.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                Self::show_contents(ui, state);
            });
    }

    fn show_contents(ui: &mut Ui, state: &mut PerfState) {
        // FPS section
        ui.heading("Frame Rate");
        ui.horizontal(|ui| {
            ui.strong("FPS:");
            let fps_color = if state.current_fps >= 55.0 {
                Color32::GREEN
            } else if state.current_fps >= 30.0 {
                Color32::YELLOW
            } else {
                Color32::RED
            };
            ui.colored_label(fps_color, format!("{:.1}", state.current_fps));
            ui.separator();
            ui.strong("Frame:");
            ui.label(format!("{:.2} ms", state.avg_frame_time()));
        });

        ui.horizontal(|ui| {
            ui.label(format!(
                "Min: {:.2} ms | Max: {:.2} ms | Peak: {:.2} ms",
                state.min_frame_time(),
                state.max_frame_time(),
                state.peak_frame_time
            ));
        });

        if ui.small_button("Reset Peak").clicked() {
            state.reset_peak();
        }

        ui.add_space(4.0);

        // Frame time graph
        Self::draw_graph(
            ui,
            "Frame Time (ms)",
            &state.frame_times,
            0.0,
            50.0, // Cap at 50ms for visibility
            |v| {
                if v < 16.67 {
                    Color32::GREEN
                } else if v < 33.33 {
                    Color32::YELLOW
                } else {
                    Color32::RED
                }
            },
        );

        ui.add_space(12.0);

        // Memory section
        ui.heading("Memory");
        let current_mem = state.memory_usage.back().copied().unwrap_or(0.0);
        let peak_mem = state.memory_usage.iter().copied().fold(0.0f32, f32::max);

        ui.horizontal(|ui| {
            ui.strong("Working Set:");
            ui.label(format!("{:.1} MB", current_mem));
            ui.separator();
            ui.strong("Peak:");
            ui.label(format!("{:.1} MB", peak_mem));
        });

        ui.add_space(4.0);

        // Memory graph
        let max_mem = (peak_mem * 1.2).max(100.0);
        Self::draw_graph(
            ui,
            "Memory (MB)",
            &state.memory_usage,
            0.0,
            max_mem,
            |_| Color32::from_rgb(100, 150, 255),
        );

        ui.add_space(12.0);

        // Stats section
        ui.heading("Statistics");
        ui.horizontal(|ui| {
            ui.strong("Total Frames:");
            ui.label(format!("{}", state.total_frames));
        });
    }

    /// Draw a line graph with the given data.
    fn draw_graph<F>(
        ui: &mut Ui,
        _label: &str,
        data: &VecDeque<f32>,
        min_val: f32,
        max_val: f32,
        color_fn: F,
    ) where
        F: Fn(f32) -> Color32,
    {
        let desired_size = Vec2::new(ui.available_width(), 80.0);
        let (rect, _response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

        if data.is_empty() {
            return;
        }

        let painter = ui.painter_at(rect);

        // Background
        painter.rect_filled(rect, 2.0, Color32::from_gray(30));

        // Grid lines
        let grid_color = Color32::from_gray(60);
        for i in 1..4 {
            let y = rect.top() + rect.height() * (i as f32 / 4.0);
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                egui::Stroke::new(1.0, grid_color),
            );
        }

        // Data points
        let range = max_val - min_val;
        if range <= 0.0 {
            return;
        }

        let step = rect.width() / (HISTORY_SIZE - 1) as f32;
        let mut points: Vec<egui::Pos2> = Vec::with_capacity(data.len());

        for (i, &val) in data.iter().enumerate() {
            let x = rect.left() + i as f32 * step;
            let normalized = ((val - min_val) / range).clamp(0.0, 1.0);
            let y = rect.bottom() - normalized * rect.height();
            points.push(egui::pos2(x, y));
        }

        // Draw filled area
        if points.len() >= 2 {
            let last_val = data.back().copied().unwrap_or(0.0);
            let fill_color = color_fn(last_val).linear_multiply(0.3);

            let mut fill_points = points.clone();
            fill_points.push(egui::pos2(points.last().unwrap().x, rect.bottom()));
            fill_points.push(egui::pos2(points.first().unwrap().x, rect.bottom()));

            painter.add(egui::Shape::convex_polygon(
                fill_points,
                fill_color,
                egui::Stroke::NONE,
            ));
        }

        // Draw line
        if points.len() >= 2 {
            let last_val = data.back().copied().unwrap_or(0.0);
            let line_color = color_fn(last_val);

            for window in points.windows(2) {
                painter.line_segment(
                    [window[0], window[1]],
                    egui::Stroke::new(2.0, line_color),
                );
            }
        }

        // Current value label
        if let Some(&last) = data.back() {
            let text = format!("{:.1}", last);
            let text_pos = egui::pos2(rect.right() - 40.0, rect.top() + 4.0);
            painter.text(
                text_pos,
                egui::Align2::LEFT_TOP,
                text,
                egui::FontId::monospace(12.0),
                Color32::WHITE,
            );
        }

        // Scale labels
        painter.text(
            egui::pos2(rect.left() + 2.0, rect.top() + 2.0),
            egui::Align2::LEFT_TOP,
            format!("{:.0}", max_val),
            egui::FontId::monospace(10.0),
            Color32::GRAY,
        );
        painter.text(
            egui::pos2(rect.left() + 2.0, rect.bottom() - 12.0),
            egui::Align2::LEFT_TOP,
            format!("{:.0}", min_val),
            egui::FontId::monospace(10.0),
            Color32::GRAY,
        );
    }
}
