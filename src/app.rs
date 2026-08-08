use crate::{
    input::{InputListener, PressedInput},
    storage::{Binding, BindingMatch, CompanionApp, Profile, Store},
};
use anyhow::{Context, Result};
use eframe::egui;
use std::{
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

pub struct WillaApp {
    store: Option<Store>,
    profiles: Vec<Profile>,
    selected: Option<usize>,
    new_name: String,
    config_path: String,
    companion_apps: Vec<CompanionApp>,
    status: Status,
    binding_view: Option<BindingView>,
    input_lookup: Option<InputLookup>,
    active_profile_dirty: bool,
    last_dirty_check: Instant,
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
    profile_names: Vec<String>,
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
        cc.egui_ctx.style_mut(|style| {
            style.spacing.button_padding = egui::vec2(10.0, 6.0);
            style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        });
        match Store::open() {
            Ok(store) => {
                let config_path = store.settings.lmu_config_path.display().to_string();
                let companion_apps = store.settings.companion_apps.clone();
                let profiles = store.profiles().unwrap_or_default();
                Self {
                    store: Some(store),
                    profiles,
                    selected: None,
                    new_name: String::new(),
                    config_path,
                    companion_apps,
                    status: Status::Ready(
                        "Choose a profile or capture LMU's current bindings.".into(),
                    ),
                    binding_view: None,
                    input_lookup: None,
                    active_profile_dirty: false,
                    last_dirty_check: Instant::now() - Duration::from_secs(2),
                }
            }
            Err(error) => Self {
                store: None,
                profiles: Vec::new(),
                selected: None,
                new_name: String::new(),
                config_path: String::new(),
                companion_apps: Vec::new(),
                status: Status::Error(error.to_string()),
                binding_view: None,
                input_lookup: None,
                active_profile_dirty: false,
                last_dirty_check: Instant::now() - Duration::from_secs(2),
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

    fn save_companion_apps(&mut self) {
        if let Some(store) = &mut self.store {
            store.settings.companion_apps = self.companion_apps.clone();
            match store.save_settings() {
                Ok(()) => self.status = Status::Success("Companion apps saved.".into()),
                Err(error) => self.status = Status::Error(error.to_string()),
            }
        }
    }

    fn copy_companion_launch_option(&mut self, ctx: &egui::Context) {
        let Some(store) = &self.store else {
            return;
        };
        match std::env::current_exe()
            .context("could not determine Willa's executable location")
            .and_then(|willa| store.companion_launch_option(&willa))
        {
            Ok(option) => {
                ctx.copy_text(option);
                self.status = Status::Success(
                    "Steam launch option copied. Paste it into LMU Properties → Launch Options."
                        .into(),
                );
            }
            Err(error) => self.status = Status::Error(error.to_string()),
        }
    }

    fn check_active_profile(&mut self) {
        if self.last_dirty_check.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_dirty_check = Instant::now();
        self.active_profile_dirty = self
            .store
            .as_ref()
            .and_then(|store| store.active_profile_has_unsaved_changes().ok().flatten())
            .unwrap_or(false);
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
                    self.active_profile_dirty = false;
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
        let profile_names = self
            .profiles
            .iter()
            .map(|profile| profile.name.clone())
            .collect();
        match InputListener::new() {
            Ok(listener) => {
                self.input_lookup = Some(InputLookup {
                    listener: Some(listener),
                    pressed: None,
                    matches: Vec::new(),
                    profile_names,
                    error: None,
                });
            }
            Err(error) => {
                self.input_lookup = Some(InputLookup {
                    listener: None,
                    pressed: None,
                    matches: Vec::new(),
                    profile_names,
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
                lookup.matches = self
                    .store
                    .as_ref()
                    .and_then(|store| {
                        store
                            .binding_matches(
                                &self.profiles,
                                pressed.vendor_id,
                                pressed.product_id,
                                pressed.input_id,
                            )
                            .ok()
                    })
                    .unwrap_or_default();
                lookup.pressed = Some(pressed);
                ctx.request_repaint();
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
        self.check_active_profile();
        ctx.request_repaint_after(Duration::from_secs(1));
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

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.add_space(10.0);
            if self.active_profile_dirty {
                ui.colored_label(
                    egui::Color32::from_rgb(235, 185, 80),
                    "Unsaved changes: LMU's bindings differ from the active profile.",
                );
                ui.weak("Save them as a profile before activating another setup.");
                ui.add_space(6.0);
            }
            let (text, color) = match &self.status {
                Status::Ready(text) => (text, ui.visuals().weak_text_color()),
                Status::Success(text) => (text, egui::Color32::from_rgb(100, 210, 140)),
                Status::Error(text) => (text, egui::Color32::from_rgb(240, 110, 110)),
            };
            ui.colored_label(color, text);
            ui.small("Close LMU before activating a profile. A recovery backup is created first.");
            ui.add_space(10.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
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
                            ui.horizontal_wrapped(|ui| {
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
                            ui.separator();
                            ui.collapsing("Launch companion apps with LMU", |ui| {
                                ui.weak("Enable each app that should start alongside LMU.");
                                let mut changed = false;
                                for app in &mut self.companion_apps {
                                    ui.horizontal(|ui| {
                                        changed |=
                                            ui.checkbox(&mut app.enabled, &app.name).changed();
                                        if ui.button("Browse…").clicked()
                                            && let Some(path) = rfd::FileDialog::new()
                                                .add_filter("Windows application", &["exe"])
                                                .pick_file()
                                        {
                                            app.path = path;
                                            app.enabled = true;
                                            changed = true;
                                        }
                                    });
                                    let path = if app.path.as_os_str().is_empty() {
                                        "No executable selected".into()
                                    } else {
                                        app.path.display().to_string()
                                    };
                                    ui.indent(("companion_path", &app.name), |ui| {
                                        ui.add(
                                            egui::Label::new(egui::RichText::new(path).weak())
                                                .wrap(),
                                        );
                                    });
                                }
                                if changed {
                                    self.save_companion_apps();
                                }
                                ui.horizontal_wrapped(|ui| {
                                    if ui.button("Add another app…").clicked()
                                        && let Some(path) = rfd::FileDialog::new()
                                            .add_filter("Windows application", &["exe"])
                                            .pick_file()
                                    {
                                        let name = path
                                            .file_stem()
                                            .and_then(|name| name.to_str())
                                            .unwrap_or("Companion app")
                                            .to_owned();
                                        self.companion_apps.push(CompanionApp {
                                            name,
                                            path,
                                            enabled: true,
                                        });
                                        self.save_companion_apps();
                                    }
                                    if ui.button("Copy Steam launch option").clicked() {
                                        self.save_companion_apps();
                                        self.copy_companion_launch_option(ctx);
                                    }
                                });
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
                            ui.heading("Saved profiles");
                            ui.horizontal_wrapped(|ui| {
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
                            if hovering_files {
                                ui.colored_label(
                                    egui::Color32::from_rgb(120, 220, 170),
                                    "Drop to import preset files or folders",
                                );
                            } else {
                                ui.weak(
                                    "Select a setup, or drop LMU preset JSON files here to import.",
                                );
                            }
                            let active_profile = self
                                .store
                                .as_ref()
                                .and_then(|store| store.settings.active_profile)
                                .and_then(|id| {
                                    self.profiles.iter().find(|profile| profile.id == id)
                                });
                            ui.horizontal_wrapped(|ui| {
                                ui.strong("Active preset:");
                                if let Some(profile) = active_profile {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(100, 210, 140),
                                        &profile.name,
                                    );
                                } else {
                                    ui.weak("None");
                                }
                                if let Some(profile) =
                                    self.selected.and_then(|i| self.profiles.get(i))
                                    && active_profile.is_none_or(|active| active.id != profile.id)
                                {
                                    ui.separator();
                                    ui.strong("Selected:");
                                    ui.label(&profile.name);
                                }
                            });
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
                            ui.horizontal_wrapped(|ui| {
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
                                    .add_enabled(
                                        self.selected.is_some(),
                                        egui::Button::new("Delete"),
                                    )
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
                            ui.weak(
                                "Configure the wheel in LMU, then save that setup with a name.",
                            );
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
                });
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
        egui::Window::new("Find a wheel button")
            .open(&mut open)
            .default_width(480.0)
            .resizable(true)
            .show(ctx, |ui| {
                if let Some(error) = &lookup.error {
                    ui.colored_label(egui::Color32::from_rgb(240, 110, 110), error);
                    return;
                }
                if lookup.listener.is_some() && lookup.pressed.is_none() {
                    ui.spinner();
                    ui.strong("Press a button, turn the wheel, or move an axis…");
                    ui.weak("Matching bindings will appear here for each saved profile.");
                    return;
                }
                let Some(pressed) = &lookup.pressed else {
                    return;
                };
                ui.heading(&pressed.control);
                ui.weak(format!(
                    "Device {:04X}:{:04X} · LMU input {}",
                    pressed.vendor_id, pressed.product_id, pressed.input_id
                ));
                ui.separator();
                if lookup.profile_names.is_empty() {
                    ui.weak("No saved profiles.");
                    return;
                }
                egui::ScrollArea::vertical()
                    .max_height(520.0)
                    .show(ui, |ui| {
                        for profile_name in &lookup.profile_names {
                            let actions = lookup
                                .matches
                                .iter()
                                .filter(|mapping| mapping.profile_name == *profile_name)
                                .map(|mapping| {
                                    if mapping.alternate {
                                        format!("{} (alternate)", mapping.action)
                                    } else {
                                        mapping.action.clone()
                                    }
                                })
                                .collect::<Vec<_>>();
                            egui::Frame::new()
                                .fill(if actions.is_empty() {
                                    egui::Color32::TRANSPARENT
                                } else {
                                    egui::Color32::from_rgb(38, 82, 63)
                                })
                                .inner_margin(egui::Margin::symmetric(8, 5))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.strong(profile_name);
                                        ui.label("→");
                                        if actions.is_empty() {
                                            ui.weak("Not mapped");
                                        } else {
                                            ui.label(actions.join(", "));
                                        }
                                    });
                                });
                        }
                    });
            });
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
