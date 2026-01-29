use egui::{Context, Color32, RichText, ScrollArea, TextStyle};
use crate::state::AppState;
use crate::cfg_window::{CfgState, CfgWindow};
use tv_core::{Architecture, DisassemblyResult, disassemble, detect_architecture, FileRegion};
use tv_core::{ControlFlowGraph, CfgInstruction};

/// State for the disassembly window.
pub struct DisasmState {
    /// Selected architecture.
    pub arch: Architecture,
    /// Auto-detect architecture from file headers.
    pub auto_detect: bool,
    /// Number of instructions to disassemble.
    pub max_instructions: usize,
    /// Current disassembly result.
    pub result: Option<DisassemblyResult>,
    /// Whether disassembly is in progress.
    pub computing: bool,
    /// Selected instruction index (for highlighting).
    pub selected_idx: Option<usize>,
    /// Follow viewport: auto-disassemble at current offset.
    pub follow_viewport: bool,
    /// Last disassembled offset (to avoid re-computing).
    cached_offset: u64,
    /// Last file size (for cache invalidation).
    cached_file_size: u64,
    /// CFG state.
    pub cfg: CfgState,
    /// Show CFG window.
    pub show_cfg: bool,
}

impl Default for DisasmState {
    fn default() -> Self {
        Self {
            arch: Architecture::X86_64,
            auto_detect: true,
            max_instructions: 100,
            result: None,
            computing: false,
            selected_idx: None,
            follow_viewport: false,
            cached_offset: u64::MAX,
            cached_file_size: 0,
            cfg: CfgState::new(),
            show_cfg: false,
        }
    }
}

impl DisasmState {
    pub fn invalidate(&mut self) {
        self.result = None;
        self.cached_offset = u64::MAX;
        self.cached_file_size = 0;
    }

    pub fn needs_recompute(&self, offset: u64, file_size: u64) -> bool {
        self.result.is_none()
            || self.cached_offset != offset
            || self.cached_file_size != file_size
    }
}

/// Floating window for disassembly view.
pub struct DisasmWindow;

impl DisasmWindow {
    pub fn show(ctx: &Context, state: &mut AppState, disasm: &mut DisasmState, visible: &mut bool) {
        if !*visible {
            return;
        }

        egui::Window::new("Disassembly")
            .open(visible)
            .default_size([600.0, 500.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                Self::show_contents(ui, state, disasm);
            });

        // Show CFG window if enabled
        CfgWindow::show(ctx, &mut disasm.cfg, &mut disasm.show_cfg);
    }

    fn show_contents(ui: &mut egui::Ui, state: &mut AppState, disasm: &mut DisasmState) {
        if !state.has_file() {
            ui.label("Open a file to disassemble.");
            return;
        }

        let file_size = state.file_len();
        let current_offset = state.viewport.start;

        // Controls bar
        ui.horizontal(|ui| {
            // Architecture selector
            ui.label("Arch:");
            let arch_label = if disasm.auto_detect {
                "Auto".to_string()
            } else {
                disasm.arch.label().to_string()
            };

            egui::ComboBox::from_id_salt("disasm_arch")
                .selected_text(arch_label)
                .show_ui(ui, |ui| {
                    if ui.selectable_value(&mut disasm.auto_detect, true, "Auto-detect").changed() {
                        disasm.invalidate();
                    }
                    ui.separator();
                    for arch in Architecture::all() {
                        let label = arch.label();
                        if ui.selectable_label(!disasm.auto_detect && disasm.arch == *arch, label).clicked() {
                            disasm.auto_detect = false;
                            disasm.arch = *arch;
                            disasm.invalidate();
                        }
                    }
                });

            ui.separator();

            // Instruction count
            ui.label("Count:");
            egui::ComboBox::from_id_salt("disasm_count")
                .selected_text(format!("{}", disasm.max_instructions))
                .width(60.0)
                .show_ui(ui, |ui| {
                    for count in [50, 100, 200, 500, 1000] {
                        if ui.selectable_value(&mut disasm.max_instructions, count, format!("{}", count)).changed() {
                            disasm.invalidate();
                        }
                    }
                });

            ui.separator();

            // Follow viewport toggle
            ui.checkbox(&mut disasm.follow_viewport, "Follow");
            if ui.button("Refresh").clicked() {
                disasm.invalidate();
            }

            ui.separator();

            // Show CFG button
            if disasm.result.is_some() {
                if ui.button("Show CFG").clicked() {
                    // Build CFG from current disassembly
                    if let Some(ref result) = disasm.result {
                        let cfg_instructions: Vec<CfgInstruction> = result.instructions.iter()
                            .map(|i| CfgInstruction {
                                address: i.address,
                                size: i.bytes.len() as u8,
                                mnemonic: i.mnemonic.clone(),
                                operands: i.operands.clone(),
                                bytes: i.bytes.clone(),
                            })
                            .collect();

                        if !cfg_instructions.is_empty() {
                            let entry = cfg_instructions[0].address;
                            disasm.cfg.cfg = Some(ControlFlowGraph::build(&cfg_instructions, entry));
                            disasm.show_cfg = true;
                        }
                    }
                }
            }
        });

        ui.separator();

        // Offset display
        ui.horizontal(|ui| {
            ui.label(format!("Offset: 0x{:X}", current_offset));
            if let Some(ref result) = disasm.result {
                ui.weak(format!("({} instructions, {} bytes)",
                    result.instructions.len(),
                    result.bytes_consumed
                ));
            }
        });

        // Auto-compute if following viewport or needs refresh
        let should_compute = disasm.follow_viewport && disasm.needs_recompute(current_offset, file_size);

        if should_compute || (disasm.result.is_none() && !disasm.computing) {
            // Perform disassembly
            if let Some(ref file) = state.file {
                let data = file.mapped.slice(FileRegion::new(current_offset, 4096.min(file_size - current_offset)));

                // Detect or use selected architecture
                let arch = if disasm.auto_detect {
                    // Try to detect from file header
                    let header = file.mapped.slice(FileRegion::new(0, 256.min(file_size)));
                    detect_architecture(header).unwrap_or(Architecture::X86_64)
                } else {
                    disasm.arch
                };

                match disassemble(data, current_offset, arch, disasm.max_instructions) {
                    Ok(result) => {
                        disasm.result = Some(result);
                        disasm.cached_offset = current_offset;
                        disasm.cached_file_size = file_size;
                    }
                    Err(e) => {
                        disasm.result = Some(DisassemblyResult {
                            arch,
                            base_address: current_offset,
                            instructions: vec![],
                            bytes_consumed: 0,
                            error: Some(e.to_string()),
                        });
                    }
                }
            }
        }

        // Show "Disassemble" button if not following
        if !disasm.follow_viewport && disasm.result.is_none() {
            ui.horizontal(|ui| {
                if ui.button("Disassemble at current offset").clicked() {
                    disasm.invalidate(); // Will trigger computation
                }
            });
        }

        // Display error if any
        if let Some(ref result) = disasm.result {
            if let Some(ref error) = result.error {
                ui.colored_label(Color32::RED, format!("Error: {}", error));
            }
        }

        ui.separator();

        // Instruction list
        if let Some(result) = disasm.result.clone() {
            let mut new_selected = disasm.selected_idx;
            let mut new_viewport: Option<u64> = None;

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let (sel, vp) = Self::show_instructions(ui, &result, disasm.selected_idx);
                    new_selected = sel;
                    new_viewport = vp;
                });

            disasm.selected_idx = new_selected;
            if let Some(offset) = new_viewport {
                state.viewport.start = offset;
            }
        }
    }

    fn show_instructions(
        ui: &mut egui::Ui,
        result: &DisassemblyResult,
        selected_idx: Option<usize>,
    ) -> (Option<usize>, Option<u64>) {
        let mut new_selected = selected_idx;
        let mut new_viewport: Option<u64> = None;

        // Use monospace font
        let mono_style = TextStyle::Monospace;

        for (idx, insn) in result.instructions.iter().enumerate() {
            let is_selected = selected_idx == Some(idx);

            let response = ui.horizontal(|ui| {
                // Address column
                let addr_text = RichText::new(format!("{:08X}", insn.address))
                    .color(Color32::from_rgb(100, 150, 200))
                    .text_style(mono_style.clone());
                ui.label(addr_text);

                ui.add_space(8.0);

                // Bytes column (fixed width)
                let bytes_hex = insn.bytes_hex();
                let bytes_text = RichText::new(format!("{:24}", bytes_hex))
                    .color(Color32::from_rgb(120, 120, 120))
                    .text_style(mono_style.clone());
                ui.label(bytes_text);

                ui.add_space(8.0);

                // Mnemonic column
                let mnemonic_color = Self::mnemonic_color(&insn.mnemonic);
                let mnemonic_text = RichText::new(format!("{:8}", insn.mnemonic))
                    .color(mnemonic_color)
                    .text_style(mono_style.clone());
                ui.label(mnemonic_text);

                // Operands column
                let operands_text = RichText::new(&insn.operands)
                    .color(Color32::from_rgb(200, 200, 200))
                    .text_style(mono_style.clone());
                ui.label(operands_text);
            });

            // Make the row clickable
            let row_rect = response.response.rect;
            let row_response = ui.interact(row_rect, ui.id().with(("insn", idx)), egui::Sense::click());

            // Highlight selected row
            if is_selected {
                ui.painter().rect_filled(
                    row_rect,
                    0.0,
                    Color32::from_rgba_unmultiplied(100, 150, 200, 30),
                );
            }

            // Handle click - navigate to instruction offset
            if row_response.clicked() {
                new_selected = Some(idx);
                new_viewport = Some((insn.address / 16) * 16);
            }

            // Hover effect
            if row_response.hovered() {
                ui.painter().rect_filled(
                    row_rect,
                    0.0,
                    Color32::from_rgba_unmultiplied(255, 255, 255, 10),
                );
            }
        }

        if result.instructions.is_empty() {
            ui.colored_label(Color32::GRAY, "No instructions decoded.");
        }

        (new_selected, new_viewport)
    }

    /// Color-code mnemonics by category.
    fn mnemonic_color(mnemonic: &str) -> Color32 {
        let m = mnemonic.to_lowercase();

        // Control flow - red/orange
        if m.starts_with("j") || m.starts_with("call") || m.starts_with("ret")
            || m == "loop" || m == "loope" || m == "loopne" || m.starts_with("b") && m.len() <= 3 {
            return Color32::from_rgb(255, 130, 100);
        }

        // Data movement - blue
        if m.starts_with("mov") || m.starts_with("lea") || m.starts_with("push")
            || m.starts_with("pop") || m.starts_with("xchg") || m.starts_with("ld") || m.starts_with("st") {
            return Color32::from_rgb(100, 180, 255);
        }

        // Arithmetic - green
        if m.starts_with("add") || m.starts_with("sub") || m.starts_with("mul") || m.starts_with("div")
            || m.starts_with("inc") || m.starts_with("dec") || m.starts_with("neg") || m.starts_with("imul") {
            return Color32::from_rgb(100, 200, 100);
        }

        // Logic - yellow
        if m.starts_with("and") || m.starts_with("or") || m.starts_with("xor") || m.starts_with("not")
            || m.starts_with("shl") || m.starts_with("shr") || m.starts_with("rol") || m.starts_with("ror") {
            return Color32::from_rgb(220, 200, 100);
        }

        // Compare/test - purple
        if m.starts_with("cmp") || m.starts_with("test") {
            return Color32::from_rgb(180, 130, 220);
        }

        // NOP/INT - gray
        if m == "nop" || m.starts_with("int") {
            return Color32::from_rgb(128, 128, 128);
        }

        // Default - white
        Color32::from_rgb(220, 220, 220)
    }
}
