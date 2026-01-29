//! Cross-References (XRefs) window.
//!
//! Shows all references to and from addresses in the disassembled code.

use egui::{Context, Color32, RichText, ScrollArea};
use tv_core::{XRefTable, XRef, XRefType, Instruction};
use crate::state::AppState;

/// State for the XRefs window.
pub struct XRefsState {
    /// XRef table built from disassembly.
    pub table: Option<XRefTable>,
    /// Currently selected target address (to show refs to).
    pub selected_target: Option<u64>,
    /// Address input text.
    pub address_input: String,
    /// Filter by XRef type.
    pub filter_calls: bool,
    pub filter_jumps: bool,
    pub filter_data: bool,
    /// Cached file size for invalidation.
    cached_file_size: u64,
    /// Cached disasm offset for invalidation.
    cached_disasm_offset: u64,
}

impl Default for XRefsState {
    fn default() -> Self {
        Self {
            table: None,
            selected_target: None,
            address_input: String::new(),
            filter_calls: true,
            filter_jumps: true,
            filter_data: true,
            cached_file_size: 0,
            cached_disasm_offset: u64::MAX,
        }
    }
}

impl XRefsState {
    /// Clear the XRef table.
    pub fn clear(&mut self) {
        self.table = None;
        self.selected_target = None;
        self.cached_file_size = 0;
        self.cached_disasm_offset = u64::MAX;
    }

    /// Build XRefs from instructions.
    pub fn build_from_instructions(&mut self, instructions: &[Instruction]) {
        self.table = Some(XRefTable::from_instructions(instructions));
    }

    /// Check if rebuild is needed.
    pub fn needs_rebuild(&self, file_size: u64, disasm_offset: u64) -> bool {
        self.table.is_none()
            || self.cached_file_size != file_size
            || self.cached_disasm_offset != disasm_offset
    }

    /// Mark as built for current context.
    pub fn mark_built(&mut self, file_size: u64, disasm_offset: u64) {
        self.cached_file_size = file_size;
        self.cached_disasm_offset = disasm_offset;
    }

    /// Set selected target from address.
    pub fn select_address(&mut self, addr: u64) {
        self.selected_target = Some(addr);
        self.address_input = format!("0x{:X}", addr);
    }
}

/// XRefs visualization window.
pub struct XRefsWindow;

impl XRefsWindow {
    pub fn show(ctx: &Context, state: &mut AppState, xrefs: &mut XRefsState, visible: &mut bool) {
        if !*visible {
            return;
        }

        egui::Window::new("Cross-References")
            .open(visible)
            .default_size([450.0, 500.0])
            .min_size([350.0, 300.0])
            .resizable(true)
            .show(ctx, |ui| {
                Self::show_contents(ui, state, xrefs);
            });
    }

    fn show_contents(ui: &mut egui::Ui, state: &mut AppState, xrefs: &mut XRefsState) {
        if !state.has_file() {
            ui.centered_and_justified(|ui| {
                ui.label("Open a file to analyze cross-references.");
            });
            return;
        }

        // Toolbar
        ui.horizontal(|ui| {
            ui.label("Address:");
            let response = ui.add(
                egui::TextEdit::singleline(&mut xrefs.address_input)
                    .desired_width(120.0)
                    .font(egui::TextStyle::Monospace)
            );

            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                // Parse and select address
                if let Some(addr) = parse_address(&xrefs.address_input) {
                    xrefs.selected_target = Some(addr);
                }
            }

            if ui.button("Go").clicked() {
                if let Some(addr) = parse_address(&xrefs.address_input) {
                    xrefs.selected_target = Some(addr);
                }
            }

            ui.separator();

            // Filters
            ui.checkbox(&mut xrefs.filter_calls, "Calls");
            ui.checkbox(&mut xrefs.filter_jumps, "Jumps");
            ui.checkbox(&mut xrefs.filter_data, "Data");
        });

        ui.separator();

        // Check if we have an XRef table
        let table = match &xrefs.table {
            Some(t) => t,
            None => {
                ui.centered_and_justified(|ui| {
                    ui.label("No cross-references available.\nUse the Disassembler (F5) first, then rebuild XRefs.");
                });
                return;
            }
        };

        // Stats
        ui.horizontal(|ui| {
            ui.label(format!("{} total references", table.total_refs));
            ui.separator();
            ui.label(format!("{} unique targets", table.refs_to.len()));
        });

        ui.separator();

        // Two-panel layout: targets list on left, refs to selected target on right
        ui.columns(2, |columns| {
            // Left panel: list of referenced addresses
            columns[0].heading("Referenced Addresses");
            ScrollArea::vertical()
                .id_salt("xref_targets")
                .auto_shrink([false, false])
                .max_height(400.0)
                .show(&mut columns[0], |ui| {
                    let mut addrs = table.referenced_addresses();

                    // Sort by reference count (most referenced first)
                    addrs.sort_by(|a, b| {
                        table.count_refs_to(*b).cmp(&table.count_refs_to(*a))
                    });

                    for addr in addrs.iter().take(200) {
                        let count = table.count_refs_to(*addr);
                        let is_selected = xrefs.selected_target == Some(*addr);

                        let text = format!("0x{:08X} ({} refs)", addr, count);
                        let label = if is_selected {
                            RichText::new(text).color(Color32::from_rgb(100, 200, 255)).strong()
                        } else {
                            RichText::new(text).color(Color32::from_rgb(180, 180, 180))
                        };

                        if ui.add(egui::Label::new(label).sense(egui::Sense::click())).clicked() {
                            xrefs.selected_target = Some(*addr);
                            xrefs.address_input = format!("0x{:X}", addr);
                        }
                    }

                    if addrs.len() > 200 {
                        ui.weak(format!("... and {} more", addrs.len() - 200));
                    }
                });

            // Right panel: refs to selected address
            columns[1].heading("References To Selected");

            if let Some(target) = xrefs.selected_target {
                columns[1].horizontal(|ui| {
                    ui.label(RichText::new(format!("0x{:08X}", target)).strong().color(Color32::from_rgb(100, 200, 255)));

                    if ui.button("Go to").on_hover_text("Navigate hex view to this address").clicked() {
                        state.viewport.start = (target / 16) * 16;
                    }
                });

                columns[1].separator();

                if let Some(refs) = table.get_refs_to(target) {
                    // Clone refs to avoid borrow issues
                    let refs: Vec<XRef> = refs.iter()
                        .filter(|r| {
                            match r.xref_type {
                                XRefType::Call => xrefs.filter_calls,
                                XRefType::Jump => xrefs.filter_jumps,
                                XRefType::Data | XRefType::Read | XRefType::Write => xrefs.filter_data,
                            }
                        })
                        .cloned()
                        .collect();

                    ScrollArea::vertical()
                        .id_salt("xref_refs")
                        .auto_shrink([false, false])
                        .max_height(350.0)
                        .show(&mut columns[1], |ui| {
                            for xref in &refs {
                                ui.horizontal(|ui| {
                                    // Type badge
                                    let (type_color, type_text) = match xref.xref_type {
                                        XRefType::Call => (Color32::from_rgb(100, 200, 100), "CALL"),
                                        XRefType::Jump => (Color32::from_rgb(255, 200, 100), "JUMP"),
                                        XRefType::Data => (Color32::from_rgb(100, 150, 255), "DATA"),
                                        XRefType::Read => (Color32::from_rgb(150, 150, 255), "READ"),
                                        XRefType::Write => (Color32::from_rgb(255, 100, 100), "WRITE"),
                                    };

                                    ui.label(RichText::new(format!("[{}]", type_text)).color(type_color).small());

                                    // From address (clickable)
                                    let from_text = format!("0x{:08X}", xref.from);
                                    if ui.add(
                                        egui::Label::new(
                                            RichText::new(&from_text)
                                                .color(Color32::from_rgb(150, 200, 255))
                                                .monospace()
                                        ).sense(egui::Sense::click())
                                    ).clicked() {
                                        // Navigate to source
                                        state.viewport.start = (xref.from / 16) * 16;
                                    }

                                    // Mnemonic
                                    ui.label(RichText::new(&xref.mnemonic).weak());
                                });
                            }

                            if refs.is_empty() {
                                ui.weak("No references match current filters");
                            }
                        });
                } else {
                    columns[1].weak("No references to this address");
                }
            } else {
                columns[1].centered_and_justified(|ui| {
                    ui.weak("Select an address from the list\nor enter one above");
                });
            }
        });
    }
}

/// Parse an address string (hex or decimal).
fn parse_address(input: &str) -> Option<u64> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }

    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else if s.chars().all(|c| c.is_ascii_hexdigit()) && s.chars().any(|c| c.is_ascii_alphabetic()) {
        u64::from_str_radix(s, 16).ok()
    } else {
        s.parse().ok()
    }
}
