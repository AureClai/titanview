//! Structure Inspector window for applying templates to binary data.

use egui::{Context, Color32, RichText, ScrollArea};
use std::collections::HashSet;
use std::path::PathBuf;
use crate::state::AppState;
use tv_core::{
    StructTemplate, TemplateResult, FieldValue, apply_template, builtin_templates, FileRegion,
    load_template_from_file, save_template_to_file, example_template_json,
};

/// State for the structure inspector window.
pub struct InspectorState {
    /// Currently selected template index.
    pub selected_template: usize,
    /// Current offset to apply template.
    pub offset: u64,
    /// Offset input text.
    pub offset_text: String,
    /// Current template result.
    pub result: Option<TemplateResult>,
    /// Selected field index (for highlighting).
    pub selected_field: Option<usize>,
    /// Available templates (builtin + custom).
    pub templates: Vec<StructTemplate>,
    /// Number of builtin templates (first N are builtin).
    pub builtin_count: usize,
    /// Auto-detect template from magic bytes.
    pub auto_detect: bool,
    /// Status message (message, is_error).
    pub status_message: Option<(String, bool)>,
    /// Whether to show the template editor dialog.
    pub show_editor: bool,
    /// Template JSON text for editor.
    pub editor_text: String,
    /// Path to custom templates directory.
    pub custom_templates_dir: Option<PathBuf>,
}

impl Default for InspectorState {
    fn default() -> Self {
        let templates = builtin_templates();
        let builtin_count = templates.len();
        Self {
            selected_template: 0,
            offset: 0,
            offset_text: "0".to_string(),
            result: None,
            selected_field: None,
            templates,
            builtin_count,
            auto_detect: true,
            status_message: None,
            show_editor: false,
            editor_text: String::new(),
            custom_templates_dir: None,
        }
    }
}

impl InspectorState {
    /// Get the currently selected template.
    pub fn current_template(&self) -> Option<&StructTemplate> {
        self.templates.get(self.selected_template)
    }

    /// Get highlight offsets for the hex view.
    pub fn highlight_offsets(&self) -> HashSet<u64> {
        let mut set = HashSet::new();

        if let (Some(result), Some(field_idx)) = (&self.result, self.selected_field) {
            if let Some((field, _)) = result.fields.get(field_idx) {
                let start = result.base_offset + field.offset as u64;
                let size = field.field_type.size();
                for i in 0..size {
                    set.insert(start + i as u64);
                }
            }
        }

        set
    }

    /// Try to auto-detect template from file data.
    pub fn auto_detect_template(&mut self, data: &[u8]) {
        // Check magic bytes for each template
        for (idx, template) in self.templates.iter().enumerate() {
            if let Some(first_field) = template.fields.first() {
                if let tv_core::FieldType::Magic(magic) = &first_field.field_type {
                    if data.len() >= magic.len() && &data[..magic.len()] == magic.as_slice() {
                        self.selected_template = idx;
                        return;
                    }
                }
            }
        }
    }

    /// Apply current template at current offset.
    pub fn apply(&mut self, data: &[u8]) {
        if let Some(template) = self.templates.get(self.selected_template) {
            let start = self.offset as usize;
            let end = (start + template.size).min(data.len());
            if start < data.len() {
                self.result = Some(apply_template(template, &data[start..end], self.offset));
            } else {
                self.result = None;
            }
        }
    }

    /// Clear the result.
    pub fn clear(&mut self) {
        self.result = None;
        self.selected_field = None;
    }

    /// Check if selected template is custom (not builtin).
    pub fn is_custom_template(&self) -> bool {
        self.selected_template >= self.builtin_count
    }

    /// Load a custom template from a file.
    pub fn load_template(&mut self, path: &std::path::Path) -> Result<(), String> {
        let template = load_template_from_file(path)?;
        self.templates.push(template);
        self.selected_template = self.templates.len() - 1;
        self.status_message = Some((format!("Loaded template from {:?}", path.file_name().unwrap_or_default()), false));
        Ok(())
    }

    /// Save a template to a file.
    pub fn save_template(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(template) = self.templates.get(self.selected_template) {
            save_template_to_file(template, path)?;
            Ok(())
        } else {
            Err("No template selected".to_string())
        }
    }

    /// Add a template from JSON text.
    pub fn add_template_from_json(&mut self, json: &str) -> Result<(), String> {
        let template = tv_core::load_template_from_json(json)?;
        let name = template.name.clone();
        self.templates.push(template);
        self.selected_template = self.templates.len() - 1;
        self.status_message = Some((format!("Added template: {}", name), false));
        Ok(())
    }

    /// Remove a custom template.
    pub fn remove_custom_template(&mut self, index: usize) {
        if index >= self.builtin_count && index < self.templates.len() {
            let name = self.templates[index].name.clone();
            self.templates.remove(index);
            if self.selected_template >= self.templates.len() {
                self.selected_template = self.templates.len().saturating_sub(1);
            }
            self.status_message = Some((format!("Removed template: {}", name), false));
            self.clear();
        }
    }

    /// Get example template JSON.
    pub fn example_json() -> String {
        example_template_json()
    }
}

/// Structure Inspector window.
pub struct StructInspector;

impl StructInspector {
    pub fn show(ctx: &Context, state: &mut AppState, inspector: &mut InspectorState, visible: &mut bool) {
        if !*visible {
            return;
        }

        egui::Window::new("Structure Inspector")
            .open(visible)
            .default_size([450.0, 500.0])
            .min_size([350.0, 300.0])
            .resizable(true)
            .show(ctx, |ui| {
                Self::show_contents(ui, state, inspector);
            });

        // Template editor dialog
        Self::show_editor_dialog(ctx, inspector);
    }

    fn show_editor_dialog(ctx: &Context, inspector: &mut InspectorState) {
        if !inspector.show_editor {
            return;
        }

        let mut close_editor = false;
        let mut add_template: Option<String> = None;
        let mut load_example = false;
        let mut clear_text = false;

        // Validate current JSON for display
        let validation_result = tv_core::load_template_from_json(&inspector.editor_text);

        egui::Window::new("Template Editor")
            .default_size([600.0, 500.0])
            .min_size([400.0, 300.0])
            .resizable(true)
            .show(ctx, |ui| {
                ui.label("Edit the JSON template below:");
                ui.add_space(5.0);

                // Help/info section
                egui::CollapsingHeader::new("JSON Format Help")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.label("Structure template JSON format:");
                        ui.add_space(3.0);
                        ui.label(RichText::new("name").strong());
                        ui.label("  Template name (string)");
                        ui.label(RichText::new("description").strong());
                        ui.label("  Template description (string)");
                        ui.label(RichText::new("little_endian").strong());
                        ui.label("  Byte order: true = LE, false = BE");
                        ui.label(RichText::new("fields").strong());
                        ui.label("  Array of field definitions");
                        ui.add_space(5.0);
                        ui.label("Field types:");
                        ui.label("  primitive: u8, u16, u32, u64, i8, i16, i32, i64, f32, f64");
                        ui.label("  byte_array: Fixed byte array with size");
                        ui.label("  string: Fixed-length string");
                        ui.label("  c_string: Null-terminated string");
                        ui.label("  magic: Expected byte sequence");
                        ui.label("  enum: Named values");
                        ui.label("  flags: Bitmask with named bits");
                    });

                ui.separator();

                // JSON editor
                ScrollArea::vertical()
                    .max_height(350.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut inspector.editor_text)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .desired_rows(20)
                        );
                    });

                ui.separator();

                // Action buttons
                ui.horizontal(|ui| {
                    if ui.button("Add Template").clicked() {
                        add_template = Some(inspector.editor_text.clone());
                    }

                    if ui.button("Load Example").clicked() {
                        load_example = true;
                    }

                    if ui.button("Clear").clicked() {
                        clear_text = true;
                    }

                    if ui.button("Cancel").clicked() {
                        close_editor = true;
                    }
                });

                // Show parse status
                match &validation_result {
                    Ok(template) => {
                        ui.label(RichText::new(format!(
                            "Valid template: {} ({} fields, {} bytes)",
                            template.name, template.fields.len(), template.size
                        )).color(Color32::from_rgb(100, 200, 100)));
                    }
                    Err(e) => {
                        ui.label(RichText::new(format!("Parse error: {}", e))
                            .color(Color32::from_rgb(255, 100, 100)));
                    }
                }
            });

        // Apply actions after the window closure
        if close_editor {
            inspector.show_editor = false;
        }

        if load_example {
            inspector.editor_text = InspectorState::example_json();
        }

        if clear_text {
            inspector.editor_text.clear();
        }

        if let Some(json) = add_template {
            match inspector.add_template_from_json(&json) {
                Ok(()) => {
                    inspector.show_editor = false;
                }
                Err(e) => {
                    inspector.status_message = Some((format!("Parse error: {}", e), true));
                }
            }
        }
    }

    fn show_contents(ui: &mut egui::Ui, state: &mut AppState, inspector: &mut InspectorState) {
        if !state.has_file() {
            ui.centered_and_justified(|ui| {
                ui.label("Open a file to inspect structures.");
            });
            return;
        }

        // Toolbar row 1: Template selector
        ui.horizontal(|ui| {
            // Template selector
            ui.label("Template:");
            let current_name = inspector.templates
                .get(inspector.selected_template)
                .map(|t| t.name.as_str())
                .unwrap_or("None");

            // Collect template info to avoid borrow issue
            let template_info: Vec<(String, bool)> = inspector.templates.iter().enumerate()
                .map(|(i, t)| (t.name.clone(), i >= inspector.builtin_count))
                .collect();
            let mut needs_clear = false;

            egui::ComboBox::from_id_salt("template_selector")
                .selected_text(current_name)
                .width(180.0)
                .show_ui(ui, |ui| {
                    for (idx, (name, is_custom)) in template_info.iter().enumerate() {
                        let label = if *is_custom {
                            RichText::new(format!("* {}", name)).color(Color32::from_rgb(100, 200, 255))
                        } else {
                            RichText::new(name.as_str())
                        };
                        if ui.selectable_value(&mut inspector.selected_template, idx, label).changed() {
                            needs_clear = true;
                        }
                    }
                });

            if needs_clear {
                inspector.clear();
            }

            // Custom template indicator
            if inspector.is_custom_template() {
                ui.label(RichText::new("(custom)").color(Color32::from_rgb(100, 200, 255)).small());
            }

            ui.checkbox(&mut inspector.auto_detect, "Auto-detect");
        });

        // Toolbar row 2: Template management
        ui.horizontal(|ui| {
            // Load template from file
            if ui.button("Load JSON...").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JSON", &["json"])
                    .add_filter("All files", &["*"])
                    .pick_file()
                {
                    match inspector.load_template(&path) {
                        Ok(()) => {}
                        Err(e) => {
                            inspector.status_message = Some((format!("Load failed: {}", e), true));
                        }
                    }
                }
            }

            // Save current template to file
            if ui.button("Save JSON...").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JSON", &["json"])
                    .set_file_name("template.json")
                    .save_file()
                {
                    match inspector.save_template(&path) {
                        Ok(()) => {
                            inspector.status_message = Some(("Template saved".to_string(), false));
                        }
                        Err(e) => {
                            inspector.status_message = Some((format!("Save failed: {}", e), true));
                        }
                    }
                }
            }

            // Open template editor
            if ui.button("New/Edit...").clicked() {
                inspector.show_editor = true;
                if inspector.editor_text.is_empty() {
                    inspector.editor_text = InspectorState::example_json();
                }
            }

            // Remove custom template
            if inspector.is_custom_template() {
                if ui.button("Remove").clicked() {
                    inspector.remove_custom_template(inspector.selected_template);
                }
            }
        });

        // Status message
        if let Some((msg, is_error)) = &inspector.status_message {
            let color = if *is_error { Color32::RED } else { Color32::from_rgb(100, 200, 100) };
            ui.label(RichText::new(msg).color(color).small());
        }

        ui.horizontal(|ui| {
            // Offset input
            ui.label("Offset:");
            let response = ui.add(
                egui::TextEdit::singleline(&mut inspector.offset_text)
                    .desired_width(120.0)
                    .font(egui::TextStyle::Monospace)
            );

            if response.changed() {
                // Parse offset
                if let Some(offset) = parse_offset(&inspector.offset_text) {
                    inspector.offset = offset;
                }
            }

            // Use current viewport button
            if ui.button("From Viewport").on_hover_text("Use current hex view offset").clicked() {
                inspector.offset = state.viewport.start;
                inspector.offset_text = format!("0x{:X}", inspector.offset);
            }

            // Apply button
            if ui.button("Apply").clicked() {
                Self::apply_template(state, inspector);
            }
        });

        // Show template description
        if let Some(template) = inspector.current_template() {
            if !template.description.is_empty() {
                ui.label(RichText::new(&template.description).weak().italics());
            }
        }

        ui.separator();

        // Results - clone to avoid borrow conflict with inspector in closure
        if let Some(result) = inspector.result.clone() {
            // Header
            ui.horizontal(|ui| {
                ui.label(RichText::new(&result.template_name).strong());
                ui.label(format!("@ 0x{:X}", result.base_offset));

                if result.magic_ok {
                    ui.label(RichText::new("Magic OK").color(Color32::from_rgb(100, 200, 100)));
                } else {
                    ui.label(RichText::new("Magic MISMATCH").color(Color32::from_rgb(255, 100, 100)));
                }
            });

            ui.separator();

            // Field list
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    Self::show_fields(ui, &result, inspector, state);
                });
        } else {
            ui.centered_and_justified(|ui| {
                ui.label("Click 'Apply' to interpret the structure at the current offset.");
            });
        }
    }

    fn apply_template(state: &AppState, inspector: &mut InspectorState) {
        if let Some(file) = &state.file {
            let file_len = file.mapped.len();

            // Auto-detect if enabled
            if inspector.auto_detect {
                let header_size = 256.min(file_len - inspector.offset.min(file_len));
                let header_data = file.mapped.slice(FileRegion::new(inspector.offset, header_size));
                inspector.auto_detect_template(header_data);
            }

            // Apply template
            if let Some(template) = inspector.current_template() {
                let data_size = (template.size as u64).min(file_len - inspector.offset.min(file_len));
                let data = file.mapped.slice(FileRegion::new(inspector.offset, data_size));
                inspector.result = Some(apply_template(template, data, inspector.offset));
            }
        }
    }

    fn show_fields(
        ui: &mut egui::Ui,
        result: &TemplateResult,
        inspector: &mut InspectorState,
        state: &mut AppState,
    ) {
        // Table header
        egui::Grid::new("struct_fields_grid")
            .num_columns(4)
            .spacing([10.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                // Header row
                ui.label(RichText::new("Offset").strong());
                ui.label(RichText::new("Field").strong());
                ui.label(RichText::new("Type").strong());
                ui.label(RichText::new("Value").strong());
                ui.end_row();

                for (idx, (field, value)) in result.fields.iter().enumerate() {
                    let is_selected = inspector.selected_field == Some(idx);
                    let abs_offset = result.base_offset + field.offset as u64;

                    // Offset column (clickable)
                    let offset_text = format!("0x{:X}", abs_offset);
                    let offset_label = if is_selected {
                        RichText::new(&offset_text).color(Color32::from_rgb(100, 200, 255)).strong()
                    } else {
                        RichText::new(&offset_text).color(Color32::from_rgb(100, 150, 200))
                    };

                    if ui.add(egui::Label::new(offset_label).sense(egui::Sense::click())).clicked() {
                        inspector.selected_field = Some(idx);
                        // Navigate hex view to this offset
                        state.viewport.start = (abs_offset / 16) * 16;
                    }

                    // Field name
                    let name_color = if is_selected {
                        Color32::from_rgb(255, 255, 100)
                    } else {
                        Color32::from_rgb(200, 200, 200)
                    };
                    let name_label = RichText::new(&field.name).color(name_color);
                    let name_response = ui.add(egui::Label::new(name_label).sense(egui::Sense::click()));

                    if name_response.clicked() {
                        inspector.selected_field = Some(idx);
                        state.viewport.start = (abs_offset / 16) * 16;
                    }

                    // Show tooltip with description
                    if let Some(desc) = &field.description {
                        name_response.on_hover_text(desc);
                    }

                    // Type column
                    let type_str = Self::type_string(&field.field_type);
                    ui.label(RichText::new(type_str).color(Color32::from_rgb(150, 150, 180)));

                    // Value column
                    let value_str = value.display();
                    let value_color = Self::value_color(value);
                    ui.label(RichText::new(value_str).color(value_color));

                    ui.end_row();
                }
            });
    }

    fn type_string(field_type: &tv_core::FieldType) -> String {
        match field_type {
            tv_core::FieldType::Primitive(p) => p.label().to_string(),
            tv_core::FieldType::ByteArray(n) => format!("[u8; {}]", n),
            tv_core::FieldType::String(n) => format!("char[{}]", n),
            tv_core::FieldType::CString(n) => format!("cstr[{}]", n),
            tv_core::FieldType::Magic(b) => format!("magic[{}]", b.len()),
            tv_core::FieldType::Enum { base, .. } => format!("enum<{}>", base.label()),
            tv_core::FieldType::Flags { base, .. } => format!("flags<{}>", base.label()),
        }
    }

    fn value_color(value: &FieldValue) -> Color32 {
        match value {
            FieldValue::Unsigned(_) => Color32::from_rgb(100, 200, 100),
            FieldValue::Signed(_) => Color32::from_rgb(100, 200, 150),
            FieldValue::Float(_) => Color32::from_rgb(200, 150, 100),
            FieldValue::Bytes(_) => Color32::from_rgb(150, 150, 150),
            FieldValue::String(_) => Color32::from_rgb(200, 200, 100),
            FieldValue::Magic { matches, .. } => {
                if *matches {
                    Color32::from_rgb(100, 255, 100)
                } else {
                    Color32::from_rgb(255, 100, 100)
                }
            }
            FieldValue::Enum { name, .. } => {
                if name.is_some() {
                    Color32::from_rgb(150, 200, 255)
                } else {
                    Color32::from_rgb(200, 150, 100)
                }
            }
            FieldValue::Flags { active, .. } => {
                if active.is_empty() {
                    Color32::from_rgb(150, 150, 150)
                } else {
                    Color32::from_rgb(200, 150, 255)
                }
            }
            FieldValue::Error(_) => Color32::from_rgb(255, 100, 100),
        }
    }
}

/// Parse an offset string (hex or decimal).
fn parse_offset(input: &str) -> Option<u64> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }

    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else if s.chars().all(|c| c.is_ascii_hexdigit()) && s.chars().any(|c| c.is_ascii_alphabetic()) {
        // Contains letters, treat as hex
        u64::from_str_radix(s, 16).ok()
    } else {
        // Try decimal
        s.parse().ok()
    }
}
