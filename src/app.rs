use crate::{
    input::{InputListener, PressedInput},
    storage::{Binding, BindingMatch, Profile, Store},
};
use anyhow::{Context, Result};
use eframe::egui;
use std::{path::PathBuf, process::Command};

pub struct WillaApp {
    store: Option<Store>,
    profiles: Vec<Profile>,
    selected: Option<usize>,
    new_name: String,
    config_path: String,
    status: Status,
    binding_view: Option<BindingView>,
    input_lookup: Option<InputLookup>,
}

struct BindingView {
    profile_name: String,
    bindings: Vec<Binding>,
    filter: String,
}

struct InputLookup {
    listener: Option<InputListener>,
    pressed: Option<PressedInput>,
    matches: Vec<BindingMatch>,
    error: Option<String>,
}

enum Status {
    Ready(String),
    Success(String),
    Error(String),
}

impl WillaApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::from_rgb(20, 22, 26);
        visuals.window_fill = visuals.panel_fill;
        cc.egui_ctx.set_visuals(visuals);
        match Store::open() {
            Ok(store) => {
                let config_path = store.settings.lmu_config_path.display().to_string();
                let profiles = store.profiles().unwrap_or_default();
                Self {
                    store: Some(store),
                    profiles,
                    selected: None,
                    new_name: String::new(),
                    config_path,
                    status: Status::Ready(
                        "Choose a profile or capture LMU's current bindings.".into(),
                    ),
                    binding_view: None,
                    input_lookup: None,
                }
            }
            Err(error) => Self {
                store: None,
                profiles: Vec::new(),
                selected: None,
                new_name: String::new(),
                config_path: String::new(),
                status: Status::Error(error.to_string()),
                binding_view: None,
                input_lookup: None,
            },
        }
    }

    fn refresh(&mut self) {
        if let Some(store) = &self.store {
            match store.profiles() {
                Ok(profiles) => self.profiles = profiles,
                Err(error) => self.status = Status::Error(error.to_string()),
            }
        }
    }

    fn save_path(&mut self) {
        if let Some(store) = &mut self.store {
            store.settings.lmu_config_path = PathBuf::from(self.config_path.trim());
            match store.save_settings() {
                Ok(()) => self.status = Status::Success("LMU config location saved.".into()),
                Err(error) => self.status = Status::Error(error.to_string()),
            }
        }
    }

    fn capture(&mut self) {
        if let Some(store) = &self.store {
            match store.capture(&self.new_name) {
                Ok(profile) => {
                    self.new_name.clear();
                    self.status = Status::Success(format!("Saved profile '{}'.", profile.name));
                    self.refresh();
                }
                Err(error) => self.status = Status::Error(error.to_string()),
            }
        }
    }

    fn show_profiles_folder(&mut self) {
        let Some(store) = &self.store else {
            return;
        };
        let path = store.profiles_dir();
        match open_folder(&path) {
            Ok(()) => {
                self.status =
                    Status::Success(format!("Opened the profiles folder: {}", path.display()));
            }
            Err(error) => self.status = Status::Error(error.to_string()),
        }
    }

    fn import_profiles(&mut self, paths: &[PathBuf]) {
        let Some(store) = &self.store else {
            return;
        };
        let mut imported = Vec::new();
        let mut failures = Vec::new();
        for path in paths {
            match store.import_profile(path) {
                Ok(profile) => imported.push(profile.name),
                Err(error) => failures.push(format!("{}: {error}", path.display())),
            }
        }
        self.refresh();
        if failures.is_empty() {
            let count = imported.len();
            self.status = Status::Success(format!(
                "Imported {count} profile{}: {}",
                if count == 1 { "" } else { "s" },
                imported.join(", ")
            ));
        } else {
            let prefix = if imported.is_empty() {
                String::new()
            } else {
                format!("Imported {}. ", imported.join(", "))
            };
            self.status =
                Status::Error(format!("{prefix}Could not import: {}", failures.join("; ")));
        }
    }

    fn activate_selected(&mut self) {
        let Some(profile) = self.selected.and_then(|i| self.profiles.get(i)).cloned() else {
            return;
        };
        if let Some(store) = &mut self.store {
            match store.activate(&profile) {
                Ok(_) => {
                    self.status = Status::Success(format!("'{}' is now active.", profile.name))
                }
                Err(error) => self.status = Status::Error(error.to_string()),
            }
        }
    }

    fn inspect_selected(&mut self) {
        let Some(profile) = self.selected.and_then(|i| self.profiles.get(i)).cloned() else {
            return;
        };
        let Some(store) = &self.store else {
            return;
        };
        match store.bindings(&profile) {
            Ok(bindings) => {
                self.binding_view = Some(BindingView {
                    profile_name: profile.name,
                    bindings,
                    filter: String::new(),
                });
            }
            Err(error) => self.status = Status::Error(error.to_string()),
        }
    }

    fn start_input_lookup(&mut self) {
        match InputListener::new() {
            Ok(listener) => {
                self.input_lookup = Some(InputLookup {
                    listener: Some(listener),
                    pressed: None,
                    matches: Vec::new(),
                    error: None,
                });
            }
            Err(error) => {
                self.input_lookup = Some(InputLookup {
                    listener: None,
                    pressed: None,
                    matches: Vec::new(),
                    error: Some(error),
                });
            }
        }
    }

    fn poll_input_lookup(&mut self, ctx: &egui::Context) {
        let Some(lookup) = &mut self.input_lookup else {
            return;
        };
        let Some(listener) = &mut lookup.listener else {
            return;
        };
        match listener.poll() {
            Ok(Some(pressed)) => {
                let input_id = pressed.button_index as u64 + 32;
                lookup.matches = self
                    .store
                    .as_ref()
                    .and_then(|store| {
                        store
                            .binding_matches(
                                &self.profiles,
                                pressed.vendor_id,
                                pressed.product_id,
                                input_id,
                            )
                            .ok()
                    })
                    .unwrap_or_default();
                lookup.pressed = Some(pressed);
                lookup.listener = None;
            }
            Ok(None) => ctx.request_repaint_after(std::time::Duration::from_millis(40)),
            Err(error) => {
                lookup.error = Some(error);
                lookup.listener = None;
            }
        }
    }

    fn delete_selected(&mut self) {
        let Some(profile) = self.selected.and_then(|i| self.profiles.get(i)).cloned() else {
            return;
        };
        if let Some(store) = &mut self.store {
            match store.delete(&profile) {
                Ok(()) => {
                    self.selected = None;
                    self.status = Status::Success(format!("Deleted '{}'.", profile.name));
                    self.refresh();
                }
                Err(error) => self.status = Status::Error(error.to_string()),
            }
        }
    }
}

impl eframe::App for WillaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_input_lookup(ctx);
        let dropped_paths = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect::<Vec<_>>()
        });
        if !dropped_paths.is_empty() {
            self.import_profiles(&dropped_paths);
        }
        let hovering_files = ctx.input(|input| !input.raw.hovered_files.is_empty());

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(14.0);
            ui.heading(egui::RichText::new("Willa").size(24.0));
            ui.weak("Le Mans Ultimate wheel profile manager");
            ui.add_space(12.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);

            egui::Frame::group(ui.style())
                .inner_margin(14.0)
                .show(ui, |ui| {
                    ui.heading("LMU configuration");
                    ui.weak("Choose the live bindings file that Willa should manage.");
                    ui.add_space(4.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.config_path)
                            .desired_width(f32::INFINITY),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Browse…").clicked()
                            && let Some(path) = rfd::FileDialog::new()
                                .add_filter("JSON", &["json"])
                                .set_file_name("direct input.json")
                                .pick_file()
                        {
                            self.config_path = path.display().to_string();
                            self.save_path();
                        }
                        if ui.button("Save path").clicked() {
                            self.save_path();
                        }
                    });
                });

            ui.add_space(10.0);
            egui::Frame::group(ui.style())
                .fill(if hovering_files {
                    egui::Color32::from_rgb(32, 53, 48)
                } else {
                    ui.visuals().faint_bg_color
                })
                .inner_margin(14.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading("Saved profiles");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(
                                    self.store.is_some(),
                                    egui::Button::new("Show in folder"),
                                )
                                .clicked()
                            {
                                self.show_profiles_folder();
                            }
                            if ui.button("Refresh").clicked() {
                                self.refresh();
                            }
                            if ui.button("Find wheel button…").clicked() {
                                self.start_input_lookup();
                            }
                        });
                    });
                    if hovering_files {
                        ui.colored_label(
                            egui::Color32::from_rgb(120, 220, 170),
                            "Drop to import preset files or folders",
                        );
                    } else {
                        ui.weak("Select a setup, or drop LMU preset JSON files here to import.");
                    }
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_height(220.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if self.profiles.is_empty() {
                                ui.weak("No profiles saved yet.");
                            }
                            let active =
                                self.store.as_ref().and_then(|s| s.settings.active_profile);
                            for (index, profile) in self.profiles.iter().enumerate() {
                                let is_selected = self.selected == Some(index);
                                let label = if active == Some(profile.id) {
                                    format!("{}  • active", profile.name)
                                } else {
                                    profile.name.clone()
                                };
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 30.0],
                                        egui::Button::new(label).selected(is_selected),
                                    )
                                    .clicked()
                                {
                                    self.selected = Some(index);
                                }
                            }
                        });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                self.selected.is_some(),
                                egui::Button::new("Activate selected"),
                            )
                            .clicked()
                        {
                            self.activate_selected();
                        }
                        if ui
                            .add_enabled(
                                self.selected.is_some(),
                                egui::Button::new("View bindings…"),
                            )
                            .clicked()
                        {
                            self.inspect_selected();
                        }
                        if ui
                            .add_enabled(self.selected.is_some(), egui::Button::new("Delete"))
                            .clicked()
                        {
                            self.delete_selected();
                        }
                    });
                });

            ui.add_space(10.0);
            egui::Frame::group(ui.style())
                .inner_margin(14.0)
                .show(ui, |ui| {
                    ui.heading("Capture current bindings");
                    ui.weak("Configure the wheel in LMU, then save that setup with a name.");
                    ui.add_space(4.0);
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.new_name)
                            .hint_text("e.g. Simagic GT Neo")
                            .desired_width(f32::INFINITY),
                    );
                    let enter = response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if ui
                        .add_sized([120.0, 30.0], egui::Button::new("Save profile"))
                        .clicked()
                        || enter
                    {
                        self.capture();
                    }
                });

            ui.add_space(12.0);
            let (text, color) = match &self.status {
                Status::Ready(text) => (text, ui.visuals().weak_text_color()),
                Status::Success(text) => (text, egui::Color32::from_rgb(100, 210, 140)),
                Status::Error(text) => (text, egui::Color32::from_rgb(240, 110, 110)),
            };
            ui.colored_label(color, text);
            ui.add_space(4.0);
            ui.small("Close LMU before activating a profile. A recovery backup is created first.");
            ui.add_space(12.0);
        });

        self.show_binding_view(ctx);
        self.show_input_lookup(ctx);
    }
}

impl WillaApp {
    fn show_binding_view(&mut self, ctx: &egui::Context) {
        let Some(view) = &mut self.binding_view else {
            return;
        };
        let mut open = true;
        egui::Window::new(format!("{} bindings", view.profile_name))
            .open(&mut open)
            .default_width(520.0)
            .default_height(560.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.weak("Search by LMU action, device, or input number.");
                ui.add(
                    egui::TextEdit::singleline(&mut view.filter)
                        .hint_text("Search bindings…")
                        .desired_width(f32::INFINITY),
                );
                ui.separator();
                let filter = view.filter.trim().to_lowercase();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut shown = 0;
                    for binding in &view.bindings {
                        let alternate = if binding.alternate { " alternate" } else { "" };
                        let searchable = format!(
                            "{} {} {}{}",
                            binding.action, binding.device, binding.input_id, alternate
                        )
                        .to_lowercase();
                        if !filter.is_empty() && !searchable.contains(&filter) {
                            continue;
                        }
                        shown += 1;
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.strong(&binding.action);
                                ui.weak(format!(
                                    "{} · input {}{alternate}",
                                    binding.device, binding.input_id
                                ));
                            });
                        });
                        ui.separator();
                    }
                    if shown == 0 {
                        ui.weak("No matching bindings.");
                    }
                });
            });
        if !open {
            self.binding_view = None;
        }
    }

    fn show_input_lookup(&mut self, ctx: &egui::Context) {
        let Some(lookup) = &mut self.input_lookup else {
            return;
        };
        let mut open = true;
        let mut listen_again = false;
        egui::Window::new("Find a wheel button")
            .open(&mut open)
            .default_width(480.0)
            .resizable(true)
            .show(ctx, |ui| {
                if lookup.listener.is_some() {
                    ui.spinner();
                    ui.strong("Press a button on your connected wheel…");
                    ui.weak("Willa will show what that button does in every saved profile.");
                    return;
                }
                if let Some(error) = &lookup.error {
                    ui.colored_label(egui::Color32::from_rgb(240, 110, 110), error);
                    if ui.button("Try again").clicked() {
                        listen_again = true;
                    }
                    return;
                }
                if let Some(pressed) = &lookup.pressed {
                    ui.heading(format!("Button {}", pressed.button_index + 1));
                    ui.weak(format!(
                        "Device {:04X}:{:04X} · LMU input {}",
                        pressed.vendor_id,
                        pressed.product_id,
                        pressed.button_index + 32
                    ));
                    ui.separator();
                    if lookup.matches.is_empty() {
                        ui.label("This button is not mapped in any saved profile.");
                    } else {
                        for mapping in &lookup.matches {
                            ui.horizontal(|ui| {
                                ui.strong(&mapping.profile_name);
                                ui.label("→");
                                ui.label(&mapping.action);
                                if mapping.alternate {
                                    ui.weak("alternate");
                                }
                            });
                        }
                    }
                    ui.add_space(8.0);
                    if ui.button("Listen for another button").clicked() {
                        listen_again = true;
                    }
                }
            });
        if listen_again {
            match InputListener::new() {
                Ok(listener) => {
                    lookup.listener = Some(listener);
                    lookup.pressed = None;
                    lookup.matches.clear();
                    lookup.error = None;
                }
                Err(error) => lookup.error = Some(error),
            }
        }
        if !open {
            self.input_lookup = None;
        }
    }
}

fn open_folder(path: &std::path::Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer.exe");
        command.arg(path);
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(path);
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };

    command
        .spawn()
        .with_context(|| format!("could not open {}", path.display()))?;
    Ok(())
}
