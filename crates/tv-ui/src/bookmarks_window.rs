//! Bookmarks and Labels management window.
//!
//! Allows users to create, edit, and navigate bookmarks and labels.

use egui::{Context, Color32, RichText, ScrollArea};
use tv_core::{Project, Bookmark, Label, LabelType};
use crate::state::AppState;
use std::path::PathBuf;

/// Current tab in the bookmarks window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BookmarksTab {
    #[default]
    Bookmarks,
    Labels,
}

/// State for the bookmarks/labels window.
pub struct BookmarksState {
    /// Current project data.
    pub project: Option<Project>,
    /// Path to the project file.
    pub project_path: Option<PathBuf>,
    /// Current tab.
    pub tab: BookmarksTab,
    /// Input for new bookmark name.
    pub new_bookmark_name: String,
    /// Input for new bookmark offset.
    pub new_bookmark_offset: String,
    /// Input for new label name.
    pub new_label_name: String,
    /// Input for new label address.
    pub new_label_address: String,
    /// Selected label type.
    pub new_label_type: LabelType,
    /// Selected bookmark index for editing.
    pub selected_bookmark: Option<usize>,
    /// Selected label index for editing.
    pub selected_label: Option<usize>,
    /// Whether project has unsaved changes.
    pub modified: bool,
    /// Status message.
    pub status_message: Option<(String, bool)>, // (message, is_error)
}

impl Default for BookmarksState {
    fn default() -> Self {
        Self {
            project: None,
            project_path: None,
            tab: BookmarksTab::Bookmarks,
            new_bookmark_name: String::new(),
            new_bookmark_offset: String::new(),
            new_label_name: String::new(),
            new_label_address: String::new(),
            new_label_type: LabelType::Unknown,
            selected_bookmark: None,
            selected_label: None,
            modified: false,
            status_message: None,
        }
    }
}

impl BookmarksState {
    /// Initialize or get the project for the current file.
    pub fn ensure_project(&mut self, file_path: &std::path::Path, file_size: u64) {
        if self.project.is_none() {
            // Try to load existing project
            let proj_path = Project::project_path_for(file_path);
            if proj_path.exists() {
                match Project::load(&proj_path) {
                    Ok(proj) => {
                        self.project = Some(proj);
                        self.project_path = Some(proj_path);
                        self.status_message = Some(("Project loaded".to_string(), false));
                        return;
                    }
                    Err(e) => {
                        log::warn!("Failed to load project: {}", e);
                    }
                }
            }
            // Create new project
            self.project = Some(Project::new(file_path, file_size));
            self.project_path = Some(proj_path);
        }
    }

    /// Save the project to disk.
    pub fn save(&mut self) -> Result<(), String> {
        let project = self.project.as_mut().ok_or("No project to save")?;
        let path = self.project_path.as_ref().ok_or("No project path")?;

        project.save(path).map_err(|e| e.to_string())?;
        self.modified = false;
        self.status_message = Some(("Project saved".to_string(), false));
        Ok(())
    }

    /// Clear the project.
    pub fn clear(&mut self) {
        self.project = None;
        self.project_path = None;
        self.modified = false;
        self.selected_bookmark = None;
        self.selected_label = None;
    }

    /// Add a bookmark at the current viewport offset.
    pub fn add_bookmark_at(&mut self, offset: u64, name: String) {
        if let Some(ref mut project) = self.project {
            project.add_bookmark(Bookmark::new(offset, name));
            self.modified = true;
        }
    }

    /// Add a label at the given address.
    pub fn add_label_at(&mut self, address: u64, name: String, label_type: LabelType) {
        if let Some(ref mut project) = self.project {
            let mut label = Label::new(address, name);
            label.label_type = label_type;
            project.add_label(label);
            self.modified = true;
        }
    }

    /// Get label at address (if any).
    pub fn get_label(&self, address: u64) -> Option<&Label> {
        self.project.as_ref()?.get_label(address)
    }

    /// Get bookmark at offset (if any).
    pub fn get_bookmark(&self, offset: u64) -> Option<&Bookmark> {
        self.project.as_ref()?.get_bookmark(offset)
    }
}

/// Bookmarks & Labels window.
pub struct BookmarksWindow;

impl BookmarksWindow {
    pub fn show(ctx: &Context, state: &mut AppState, bookmarks: &mut BookmarksState, visible: &mut bool) {
        if !*visible {
            return;
        }

        egui::Window::new("Bookmarks & Labels")
            .open(visible)
            .default_size([500.0, 450.0])
            .min_size([400.0, 300.0])
            .resizable(true)
            .show(ctx, |ui| {
                Self::show_contents(ui, state, bookmarks);
            });
    }

    fn show_contents(ui: &mut egui::Ui, state: &mut AppState, bookmarks: &mut BookmarksState) {
        if !state.has_file() {
            ui.centered_and_justified(|ui| {
                ui.label("Open a file to manage bookmarks and labels.");
            });
            return;
        }

        // Ensure project exists
        if let Some(ref file) = state.file {
            bookmarks.ensure_project(&file.path, file.mapped.len());
        }

        // Toolbar
        ui.horizontal(|ui| {
            // Tab selector
            ui.selectable_value(&mut bookmarks.tab, BookmarksTab::Bookmarks, "Bookmarks");
            ui.selectable_value(&mut bookmarks.tab, BookmarksTab::Labels, "Labels");

            ui.separator();

            // Save button
            let save_text = if bookmarks.modified { "Save *" } else { "Save" };
            if ui.button(save_text).clicked() {
                if let Err(e) = bookmarks.save() {
                    bookmarks.status_message = Some((format!("Save failed: {}", e), true));
                }
            }

            // Stats
            if let Some(ref project) = bookmarks.project {
                let stats = project.stats();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(format!("{} bookmarks, {} labels", stats.bookmarks, stats.labels));
                });
            }
        });

        // Status message
        if let Some((msg, is_error)) = &bookmarks.status_message {
            let color = if *is_error { Color32::RED } else { Color32::from_rgb(100, 200, 100) };
            ui.label(RichText::new(msg).color(color).small());
        }

        ui.separator();

        match bookmarks.tab {
            BookmarksTab::Bookmarks => Self::show_bookmarks(ui, state, bookmarks),
            BookmarksTab::Labels => Self::show_labels(ui, state, bookmarks),
        }
    }

    fn show_bookmarks(ui: &mut egui::Ui, state: &mut AppState, bookmarks: &mut BookmarksState) {
        // Add bookmark section
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.add(
                egui::TextEdit::singleline(&mut bookmarks.new_bookmark_name)
                    .desired_width(150.0)
            );

            ui.label("Offset:");
            ui.add(
                egui::TextEdit::singleline(&mut bookmarks.new_bookmark_offset)
                    .desired_width(100.0)
                    .font(egui::TextStyle::Monospace)
            );

            if ui.button("Current").on_hover_text("Use current viewport offset").clicked() {
                bookmarks.new_bookmark_offset = format!("0x{:X}", state.viewport.start);
            }

            if ui.button("Add").clicked() {
                if let Some(offset) = parse_offset(&bookmarks.new_bookmark_offset) {
                    let name = if bookmarks.new_bookmark_name.is_empty() {
                        format!("Bookmark @ 0x{:X}", offset)
                    } else {
                        bookmarks.new_bookmark_name.clone()
                    };
                    bookmarks.add_bookmark_at(offset, name);
                    bookmarks.new_bookmark_name.clear();
                    bookmarks.new_bookmark_offset.clear();
                }
            }
        });

        ui.separator();

        // Bookmark list
        let project = match &bookmarks.project {
            Some(p) => p.clone(),
            None => return,
        };

        if project.bookmarks.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.weak("No bookmarks yet.\nAdd one using the form above.");
            });
            return;
        }

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut to_delete: Option<u64> = None;
                let mut to_navigate: Option<u64> = None;

                for (idx, bookmark) in project.bookmarks.iter().enumerate() {
                    let is_selected = bookmarks.selected_bookmark == Some(idx);

                    ui.horizontal(|ui| {
                        // Selection indicator
                        let text = format!("0x{:08X}", bookmark.offset);
                        let offset_label = if is_selected {
                            RichText::new(&text).color(Color32::from_rgb(100, 200, 255)).strong().monospace()
                        } else {
                            RichText::new(&text).color(Color32::from_rgb(150, 200, 255)).monospace()
                        };

                        if ui.add(egui::Label::new(offset_label).sense(egui::Sense::click())).clicked() {
                            bookmarks.selected_bookmark = Some(idx);
                            to_navigate = Some(bookmark.offset);
                        }

                        // Name
                        let name_label = if is_selected {
                            RichText::new(&bookmark.name).strong()
                        } else {
                            RichText::new(&bookmark.name)
                        };
                        ui.label(name_label);

                        // Spacer
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("X").on_hover_text("Delete").clicked() {
                                to_delete = Some(bookmark.offset);
                            }
                            if ui.small_button("Go").on_hover_text("Navigate to offset").clicked() {
                                to_navigate = Some(bookmark.offset);
                            }
                        });
                    });
                }

                // Apply deletions
                if let Some(offset) = to_delete {
                    if let Some(ref mut project) = bookmarks.project {
                        project.remove_bookmark(offset);
                        bookmarks.modified = true;
                        bookmarks.selected_bookmark = None;
                    }
                }

                // Apply navigation
                if let Some(offset) = to_navigate {
                    state.viewport.start = (offset / 16) * 16;
                }
            });
    }

    fn show_labels(ui: &mut egui::Ui, state: &mut AppState, bookmarks: &mut BookmarksState) {
        // Add label section
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.add(
                egui::TextEdit::singleline(&mut bookmarks.new_label_name)
                    .desired_width(120.0)
            );

            ui.label("Address:");
            ui.add(
                egui::TextEdit::singleline(&mut bookmarks.new_label_address)
                    .desired_width(100.0)
                    .font(egui::TextStyle::Monospace)
            );

            ui.label("Type:");
            egui::ComboBox::from_id_salt("label_type")
                .selected_text(bookmarks.new_label_type.label())
                .width(80.0)
                .show_ui(ui, |ui| {
                    for lt in LabelType::all() {
                        ui.selectable_value(&mut bookmarks.new_label_type, *lt, lt.label());
                    }
                });

            if ui.button("Add").clicked() {
                if let Some(addr) = parse_offset(&bookmarks.new_label_address) {
                    if !bookmarks.new_label_name.is_empty() {
                        bookmarks.add_label_at(addr, bookmarks.new_label_name.clone(), bookmarks.new_label_type);
                        bookmarks.new_label_name.clear();
                        bookmarks.new_label_address.clear();
                    }
                }
            }
        });

        ui.separator();

        // Label list
        let project = match &bookmarks.project {
            Some(p) => p.clone(),
            None => return,
        };

        if project.labels.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.weak("No labels yet.\nAdd one using the form above.");
            });
            return;
        }

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut to_delete: Option<u64> = None;
                let mut to_navigate: Option<u64> = None;

                for (idx, label) in project.labels.iter().enumerate() {
                    let is_selected = bookmarks.selected_label == Some(idx);

                    ui.horizontal(|ui| {
                        // Type badge
                        let type_color = match label.label_type {
                            LabelType::Function => Color32::from_rgb(100, 200, 100),
                            LabelType::Data => Color32::from_rgb(200, 200, 100),
                            LabelType::String => Color32::from_rgb(200, 150, 100),
                            LabelType::Code => Color32::from_rgb(100, 150, 255),
                            LabelType::Import => Color32::from_rgb(200, 100, 200),
                            LabelType::Export => Color32::from_rgb(100, 200, 200),
                            LabelType::Unknown => Color32::GRAY,
                        };
                        ui.label(RichText::new(format!("[{}]", label.label_type.label())).color(type_color).small());

                        // Address
                        let addr_text = format!("0x{:08X}", label.address);
                        let addr_label = if is_selected {
                            RichText::new(&addr_text).color(Color32::from_rgb(100, 200, 255)).strong().monospace()
                        } else {
                            RichText::new(&addr_text).color(Color32::from_rgb(150, 200, 255)).monospace()
                        };

                        if ui.add(egui::Label::new(addr_label).sense(egui::Sense::click())).clicked() {
                            bookmarks.selected_label = Some(idx);
                            to_navigate = Some(label.address);
                        }

                        // Name
                        let name_label = if is_selected {
                            RichText::new(&label.name).strong()
                        } else {
                            RichText::new(&label.name)
                        };
                        ui.label(name_label);

                        // Buttons
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("X").on_hover_text("Delete").clicked() {
                                to_delete = Some(label.address);
                            }
                            if ui.small_button("Go").on_hover_text("Navigate to address").clicked() {
                                to_navigate = Some(label.address);
                            }
                        });
                    });
                }

                // Apply deletions
                if let Some(addr) = to_delete {
                    if let Some(ref mut project) = bookmarks.project {
                        project.remove_label(addr);
                        bookmarks.modified = true;
                        bookmarks.selected_label = None;
                    }
                }

                // Apply navigation
                if let Some(addr) = to_navigate {
                    state.viewport.start = (addr / 16) * 16;
                }
            });
    }
}

/// Parse an offset/address string (hex or decimal).
fn parse_offset(input: &str) -> Option<u64> {
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
