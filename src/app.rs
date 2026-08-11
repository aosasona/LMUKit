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

pub struct LmuKitApp {
    store: Option<Store>,
    active_tab: AppTab,
    profiles: Vec<Profile>,
    selected: Option<usize>,
    new_name: String,
    config_path: String,
    companion_apps: Vec<CompanionApp>,
    status: Status,
    binding_view: Option<BindingView>,
    input_lookup: Option<InputLookup>,
    profile_editor: Option<ProfileEditor>,
    discard_prompt: Option<DiscardAction>,
    active_profile_dirty: bool,
    last_dirty_check: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppTab {
    Profiles,
    Editor,
    Settings,
}

struct ProfileEditor {
    profile: Profile,
    original: serde_json::Value,
    document: serde_json::Value,
    known_actions: Vec<String>,
    section: EditorSection,
    filter: String,
    listener: Option<InputListener>,
    listening_for: Option<BindingTarget>,
    input_error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditorSection {
    ForceFeedback,
    Bindings,
    Json,
}

#[derive(Clone)]
struct BindingTarget {
    action: String,
    alternate: bool,
}

enum ProfileEditorAction {
    Listen(BindingTarget),
    Clear(BindingTarget),
}

struct JsonSetting {
    path: Vec<String>,
    label: String,
}

#[derive(Clone, Copy)]
enum DiscardAction {
    SwitchTab(AppTab),
    CloseApp,
}

struct BindingView {
    profile: Profile,
    profile_name: String,
    bindings: Vec<Binding>,
    filter: String,
    listener: Option<InputListener>,
    pressed: Option<PressedInput>,
    pressed_actions: Vec<String>,
    input_error: Option<String>,
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

impl LmuKitApp {
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
                    active_tab: AppTab::Profiles,
                    profiles,
                    selected: None,
                    new_name: String::new(),
                    config_path,
                    companion_apps,
                    status: Status::Ready("Choose a tool to get started.".into()),
                    binding_view: None,
                    input_lookup: None,
                    profile_editor: None,
                    discard_prompt: None,
                    active_profile_dirty: false,
                    last_dirty_check: Instant::now() - Duration::from_secs(2),
                }
            }
            Err(error) => Self {
                store: None,
                active_tab: AppTab::Profiles,
                profiles: Vec::new(),
                selected: None,
                new_name: String::new(),
                config_path: String::new(),
                companion_apps: Vec::new(),
                status: Status::Error(error.to_string()),
                binding_view: None,
                input_lookup: None,
                profile_editor: None,
                discard_prompt: None,
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
            .context("could not determine LMUKit's executable location")
            .and_then(|lmukit| store.companion_launch_option(&lmukit))
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

    fn update_profile_from_live(&mut self, profile: Profile) {
        let Some(store) = &self.store else {
            return;
        };
        match store.update_profile(&profile) {
            Ok(backup) => {
                if store.settings.active_profile == Some(profile.id) {
                    self.active_profile_dirty = false;
                }
                if self
                    .binding_view
                    .as_ref()
                    .is_some_and(|view| view.profile.id == profile.id)
                {
                    self.binding_view = None;
                }
                self.status = Status::Success(format!(
                    "Updated '{}'. Previous version backed up to {}.",
                    profile.name,
                    backup.display()
                ));
            }
            Err(error) => self.status = Status::Error(error.to_string()),
        }
    }

    fn update_selected_from_live(&mut self) {
        if let Some(profile) = self.selected.and_then(|i| self.profiles.get(i)).cloned() {
            self.update_profile_from_live(profile);
        }
    }

    fn update_active_from_live(&mut self) {
        let active = self
            .store
            .as_ref()
            .and_then(|store| store.settings.active_profile);
        if let Some(profile) = active
            .and_then(|id| self.profiles.iter().find(|profile| profile.id == id))
            .cloned()
        {
            self.update_profile_from_live(profile);
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
                let (listener, input_error) = match InputListener::new() {
                    Ok(listener) => (Some(listener), None),
                    Err(error) => (None, Some(error)),
                };
                self.binding_view = Some(BindingView {
                    profile_name: profile.name.clone(),
                    profile,
                    bindings,
                    filter: String::new(),
                    listener,
                    pressed: None,
                    pressed_actions: Vec::new(),
                    input_error,
                });
            }
            Err(error) => self.status = Status::Error(error.to_string()),
        }
    }

    fn edit_selected(&mut self) {
        let Some(profile) = self.selected.and_then(|i| self.profiles.get(i)).cloned() else {
            return;
        };
        let Some(store) = &self.store else {
            return;
        };
        match store.load_profile_document(&profile) {
            Ok(document) => {
                let mut known_actions: Vec<String> = Store::bindings_from_document(&document)
                    .into_iter()
                    .map(|binding| binding.action)
                    .collect();
                known_actions.extend(profile.assignments.iter().cloned());
                known_actions.sort_by_key(|action| action.to_lowercase());
                known_actions.dedup();
                self.profile_editor = Some(ProfileEditor {
                    profile,
                    original: document.clone(),
                    document,
                    known_actions,
                    section: EditorSection::Bindings,
                    filter: String::new(),
                    listener: None,
                    listening_for: None,
                    input_error: None,
                });
                self.active_tab = AppTab::Editor;
            }
            Err(error) => self.status = Status::Error(error.to_string()),
        }
    }

    fn editor_is_dirty(&self) -> bool {
        self.profile_editor
            .as_ref()
            .is_some_and(|editor| editor.document != editor.original)
    }

    fn save_editor(&mut self, activate: bool) -> bool {
        let Some(editor) = &self.profile_editor else {
            return false;
        };
        let mut profile = editor.profile.clone();
        profile.assignments = editor.known_actions.clone();
        let document = editor.document.clone();
        let Some(store) = &mut self.store else {
            return false;
        };
        if document == editor.original {
            if activate {
                match store.activate(&profile) {
                    Ok(_) => {
                        self.active_profile_dirty = false;
                        self.status = Status::Success(format!("'{}' is now active.", profile.name));
                    }
                    Err(error) => {
                        self.status = Status::Error(error.to_string());
                        return false;
                    }
                }
            }
            return true;
        }
        let backup = match store.save_profile_document(&profile, &document) {
            Ok(backup) => backup,
            Err(error) => {
                self.status = Status::Error(error.to_string());
                return false;
            }
        };
        if activate && let Err(error) = store.activate(&profile) {
            self.status = Status::Error(format!(
                "The profile was saved, but could not be activated: {error}"
            ));
            if let Some(editor) = &mut self.profile_editor {
                editor.original = document;
            }
            return false;
        }
        if let Some(editor) = &mut self.profile_editor {
            editor.original = document;
            editor.profile = profile.clone();
        }
        if let Some(saved) = self
            .profiles
            .iter_mut()
            .find(|saved| saved.id == profile.id)
        {
            *saved = profile.clone();
        }
        if self
            .binding_view
            .as_ref()
            .is_some_and(|view| view.profile.id == profile.id)
        {
            self.binding_view = None;
        }
        if activate {
            self.active_profile_dirty = false;
        }
        self.status = Status::Success(format!(
            "Saved '{}'. Previous version backed up to {}{}",
            profile.name,
            backup.display(),
            if activate { " and activated." } else { "." }
        ));
        true
    }

    fn request_tab(&mut self, tab: AppTab) {
        if self.active_tab == AppTab::Editor && tab != AppTab::Editor && self.editor_is_dirty() {
            self.discard_prompt = Some(DiscardAction::SwitchTab(tab));
        } else {
            self.active_tab = tab;
        }
    }

    fn finish_discard_action(&mut self, action: DiscardAction, ctx: &egui::Context) {
        self.profile_editor = None;
        self.discard_prompt = None;
        match action {
            DiscardAction::SwitchTab(tab) => self.active_tab = tab,
            DiscardAction::CloseApp => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }

    fn start_editor_binding_listener(&mut self, target: BindingTarget) {
        let Some(editor) = &mut self.profile_editor else {
            return;
        };
        match InputListener::new() {
            Ok(listener) => {
                editor.listener = Some(listener);
                editor.listening_for = Some(target);
                editor.input_error = None;
            }
            Err(error) => editor.input_error = Some(error),
        }
    }

    fn poll_profile_editor(&mut self, ctx: &egui::Context) {
        let Some(editor) = &mut self.profile_editor else {
            return;
        };
        let (Some(listener), Some(target)) = (&mut editor.listener, editor.listening_for.clone())
        else {
            return;
        };
        match listener.poll() {
            Ok(Some(pressed)) => {
                if let Some(device) =
                    Store::matching_device(&editor.document, pressed.vendor_id, pressed.product_id)
                {
                    match Store::set_binding(
                        &mut editor.document,
                        &target.action,
                        target.alternate,
                        &device,
                        pressed.input_id,
                    ) {
                        Ok(()) => editor.input_error = None,
                        Err(error) => editor.input_error = Some(error.to_string()),
                    }
                } else {
                    editor.input_error = Some(
                        "That device is not present in this profile, so LMUKit cannot safely add it."
                            .into(),
                    );
                }
                editor.listener = None;
                editor.listening_for = None;
                ctx.request_repaint();
            }
            Ok(None) => ctx.request_repaint_after(Duration::from_millis(40)),
            Err(error) => {
                editor.input_error = Some(error);
                editor.listener = None;
                editor.listening_for = None;
            }
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

    fn poll_binding_view(&mut self, ctx: &egui::Context) {
        let Some(view) = &mut self.binding_view else {
            return;
        };
        let Some(listener) = &mut view.listener else {
            return;
        };
        match listener.poll() {
            Ok(Some(pressed)) => {
                view.pressed_actions = self
                    .store
                    .as_ref()
                    .and_then(|store| {
                        store
                            .binding_matches(
                                std::slice::from_ref(&view.profile),
                                pressed.vendor_id,
                                pressed.product_id,
                                pressed.input_id,
                            )
                            .ok()
                    })
                    .unwrap_or_default()
                    .into_iter()
                    .map(|mapping| {
                        if mapping.alternate {
                            format!("{} (alternate)", mapping.action)
                        } else {
                            mapping.action
                        }
                    })
                    .collect();
                view.pressed = Some(pressed);
                ctx.request_repaint();
            }
            Ok(None) => ctx.request_repaint_after(Duration::from_millis(40)),
            Err(error) => {
                view.input_error = Some(error);
                view.listener = None;
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

impl eframe::App for LmuKitApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.check_active_profile();
        ctx.request_repaint_after(Duration::from_secs(1));
        self.poll_input_lookup(ctx);
        self.poll_binding_view(ctx);
        self.poll_profile_editor(ctx);
        if ctx.input(|input| input.viewport().close_requested()) && self.editor_is_dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.discard_prompt = Some(DiscardAction::CloseApp);
        }
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
            self.request_tab(AppTab::Profiles);
        }
        let hovering_files = ctx.input(|input| !input.raw.hovered_files.is_empty());

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(14.0);
            ui.heading(egui::RichText::new("LMUKit").size(24.0));
            ui.weak("Tools for Le Mans Ultimate");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(self.active_tab == AppTab::Profiles, "Profiles")
                    .clicked()
                {
                    self.request_tab(AppTab::Profiles);
                }
                if self.profile_editor.is_some()
                    && ui
                        .selectable_label(self.active_tab == AppTab::Editor, "Editor")
                        .clicked()
                {
                    self.request_tab(AppTab::Editor);
                }
                if ui
                    .selectable_label(self.active_tab == AppTab::Settings, "Settings")
                    .clicked()
                {
                    self.request_tab(AppTab::Settings);
                }
            });
            ui.add_space(8.0);
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.add_space(10.0);
            if self.active_tab == AppTab::Profiles && self.active_profile_dirty {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(235, 185, 80),
                        "Unsaved changes: LMU's bindings differ from the active profile.",
                    );
                    if ui.button("Update active profile").clicked() {
                        self.update_active_from_live();
                    }
                });
                ui.weak("Update the active profile or capture the changes as a new profile.");
                ui.add_space(6.0);
            }
            let (text, color) = match &self.status {
                Status::Ready(text) => (text, ui.visuals().weak_text_color()),
                Status::Success(text) => (text, egui::Color32::from_rgb(100, 210, 140)),
                Status::Error(text) => (text, egui::Color32::from_rgb(240, 110, 110)),
            };
            ui.colored_label(color, text);
            if self.active_tab == AppTab::Profiles {
                ui.small(
                    "Close LMU before activating a profile. A recovery backup is created first.",
                );
            }
            ui.add_space(10.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.active_tab == AppTab::Editor {
                self.show_profile_editor(ui);
                return;
            }
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if self.active_tab == AppTab::Settings {
                        egui::Frame::group(ui.style())
                            .inner_margin(14.0)
                            .show(ui, |ui| {
                                ui.heading("LMU paths");
                                ui.weak("Choose the live bindings file that LMUKit should manage.");
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
                        ui.add_space(12.0);
                    }

                    if self.active_tab == AppTab::Profiles {
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
                                        && active_profile
                                            .is_none_or(|active| active.id != profile.id)
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
                                        let active = self
                                            .store
                                            .as_ref()
                                            .and_then(|s| s.settings.active_profile);
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
                                            egui::Button::new("Update selected from LMU"),
                                        )
                                        .on_hover_text(
                                            "Replace this saved profile with LMU's current bindings",
                                        )
                                        .clicked()
                                    {
                                        self.update_selected_from_live();
                                    }
                                    if ui
                                        .add_enabled(
                                            self.selected.is_some(),
                                            egui::Button::new("Edit profile…"),
                                        )
                                        .clicked()
                                    {
                                        self.edit_selected();
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
                    }

                    ui.add_space(12.0);
                });
        });

        self.show_binding_view(ctx);
        self.show_input_lookup(ctx);
        self.show_discard_prompt(ctx);
    }
}

impl LmuKitApp {
    fn show_profile_editor(&mut self, ui: &mut egui::Ui) {
        let mut save = false;
        let mut save_and_activate = false;
        let mut close = false;
        let mut editor_action = None;
        let Some(editor) = &mut self.profile_editor else {
            self.active_tab = AppTab::Profiles;
            return;
        };
        let dirty = editor.document != editor.original;

        ui.horizontal_wrapped(|ui| {
            ui.heading(format!("Edit {}", editor.profile.name));
            if dirty {
                ui.colored_label(egui::Color32::from_rgb(235, 185, 80), "Unsaved changes");
            } else {
                ui.weak("Saved");
            }
            ui.separator();
            save = ui.add_enabled(dirty, egui::Button::new("Save")).clicked();
            save_and_activate = ui.button("Save and activate").clicked();
            close = ui.button("Close editor").clicked();
        });
        ui.weak("Changes affect this saved profile only until you choose Save and activate.");
        ui.separator();

        egui::SidePanel::left("profile_editor_sections")
            .resizable(false)
            .exact_width(150.0)
            .show_inside(ui, |ui| {
                ui.strong("Profile sections");
                ui.add_space(6.0);
                ui.selectable_value(
                    &mut editor.section,
                    EditorSection::ForceFeedback,
                    "Force feedback",
                );
                ui.selectable_value(&mut editor.section, EditorSection::Bindings, "Bindings");
                ui.selectable_value(&mut editor.section, EditorSection::Json, "JSON preview");
            });
        egui::CentralPanel::default().show_inside(ui, |ui| match editor.section {
            EditorSection::ForceFeedback => show_force_feedback_editor(editor, ui),
            EditorSection::Bindings => {
                editor_action = show_bindings_editor(editor, ui);
            }
            EditorSection::Json => show_json_preview(editor, ui),
        });

        if let Some(action) = editor_action {
            match action {
                ProfileEditorAction::Listen(target) => {
                    self.start_editor_binding_listener(target);
                }
                ProfileEditorAction::Clear(target) => {
                    if let Some(editor) = &mut self.profile_editor
                        && let Err(error) = Store::clear_binding(
                            &mut editor.document,
                            &target.action,
                            target.alternate,
                        )
                    {
                        editor.input_error = Some(error.to_string());
                    }
                }
            }
        }
        if save {
            self.save_editor(false);
        }
        if save_and_activate {
            self.save_editor(true);
        }
        if close {
            self.request_tab(AppTab::Profiles);
            if !self.editor_is_dirty() {
                self.profile_editor = None;
            }
        }
    }

    fn show_discard_prompt(&mut self, ctx: &egui::Context) {
        let Some(action) = self.discard_prompt else {
            return;
        };
        let mut save = false;
        let mut discard = false;
        let mut cancel = false;
        egui::Window::new("Unsaved profile changes")
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Save your profile changes before leaving the editor?");
                ui.weak("Discard keeps the last saved version and loses these edits.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    save = ui.button("Save").clicked();
                    discard = ui.button("Discard").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        let proceed = discard || (save && self.save_editor(false));
        if proceed {
            self.finish_discard_action(action, ctx);
        } else if cancel {
            self.discard_prompt = None;
        }
    }

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
                let list_height = (ui.available_height() - 72.0).max(120.0);
                egui::ScrollArea::vertical()
                    .max_height(list_height)
                    .show(ui, |ui| {
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
                ui.separator();
                if let Some(error) = &view.input_error {
                    ui.colored_label(egui::Color32::from_rgb(240, 110, 110), error);
                } else if let Some(pressed) = &view.pressed {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(&pressed.control);
                        ui.label("→");
                        if view.pressed_actions.is_empty() {
                            ui.weak("Not mapped in this profile");
                        } else {
                            ui.colored_label(
                                egui::Color32::from_rgb(100, 210, 140),
                                view.pressed_actions.join(", "),
                            );
                        }
                    });
                    ui.weak(format!("LMU input {}", pressed.input_id));
                } else {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.weak("Press a button, turn the wheel, or move an axis…");
                    });
                }
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

fn show_force_feedback_editor(editor: &mut ProfileEditor, ui: &mut egui::Ui) {
    ui.heading("Force feedback");
    ui.weak(
        "LMUKit detects editable FFB values already present in this profile and preserves every unknown field.",
    );
    ui.add_space(6.0);
    let settings = collect_ffb_settings(&editor.document);
    if settings.is_empty() {
        ui.colored_label(
            egui::Color32::from_rgb(235, 185, 80),
            "No recognised FFB settings were found in this preset.",
        );
        ui.weak(
            "Capture a current LMU wheel profile first. LMUKit will not invent undocumented JSON fields.",
        );
        return;
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        for setting in settings {
            egui::Frame::group(ui.style())
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.vertical(|ui| {
                            ui.strong(&setting.label);
                            let context = setting.path[..setting.path.len() - 1].join(" › ");
                            if !context.is_empty() {
                                ui.weak(context);
                            }
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let Some(value) = json_value_mut(&mut editor.document, &setting.path)
                            else {
                                return;
                            };
                            if let Some(current) = value.as_bool() {
                                let mut edited = current;
                                if ui.checkbox(&mut edited, "").changed() {
                                    *value = serde_json::Value::Bool(edited);
                                }
                            } else if let Some(current) = value.as_f64() {
                                let mut edited = current;
                                if ui
                                    .add(egui::DragValue::new(&mut edited).speed(0.01))
                                    .changed()
                                    && let Some(number) = serde_json::Number::from_f64(edited)
                                {
                                    *value = serde_json::Value::Number(number);
                                }
                            }
                        });
                    });
                });
            ui.add_space(5.0);
        }
    });
}

fn show_bindings_editor(
    editor: &mut ProfileEditor,
    ui: &mut egui::Ui,
) -> Option<ProfileEditorAction> {
    let mut action = None;
    ui.heading("Bindings");
    ui.weak("Listen for a control to replace a primary or alternate mapping.");
    ui.add(
        egui::TextEdit::singleline(&mut editor.filter)
            .hint_text("Search actions…")
            .desired_width(f32::INFINITY),
    );
    if let Some(target) = &editor.listening_for {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.strong(format!(
                "Listening for {} {}…",
                target.action,
                if target.alternate {
                    "alternate"
                } else {
                    "primary"
                }
            ));
        });
    }
    if let Some(error) = &editor.input_error {
        ui.colored_label(egui::Color32::from_rgb(240, 110, 110), error);
    }
    ui.separator();

    let bindings = Store::bindings_from_document(&editor.document);
    let filter = editor.filter.trim().to_lowercase();
    egui::ScrollArea::vertical().show(ui, |ui| {
        for binding_action in &editor.known_actions {
            if !filter.is_empty() && !binding_action.to_lowercase().contains(&filter) {
                continue;
            }
            let primary = bindings
                .iter()
                .find(|binding| binding.action == *binding_action && !binding.alternate);
            let alternate = bindings
                .iter()
                .find(|binding| binding.action == *binding_action && binding.alternate);
            egui::Frame::group(ui.style())
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.strong(binding_action);
                    for (label, binding, is_alternate) in
                        [("Primary", primary, false), ("Alternate", alternate, true)]
                    {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(format!("{label}:"));
                            if let Some(binding) = binding {
                                let duplicate_count = bindings
                                    .iter()
                                    .filter(|other| {
                                        other.device == binding.device
                                            && other.input_id == binding.input_id
                                    })
                                    .count();
                                ui.monospace(format!(
                                    "{} · input {}",
                                    binding.device, binding.input_id
                                ));
                                if duplicate_count > 1 {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(235, 185, 80),
                                        "duplicate",
                                    );
                                }
                            } else {
                                ui.weak("Unassigned");
                            }
                            if ui
                                .add_enabled(editor.listener.is_none(), egui::Button::new("Listen"))
                                .clicked()
                            {
                                action = Some(ProfileEditorAction::Listen(BindingTarget {
                                    action: binding_action.clone(),
                                    alternate: is_alternate,
                                }));
                            }
                            if ui
                                .add_enabled(binding.is_some(), egui::Button::new("Clear"))
                                .clicked()
                            {
                                action = Some(ProfileEditorAction::Clear(BindingTarget {
                                    action: binding_action.clone(),
                                    alternate: is_alternate,
                                }));
                            }
                        });
                    }
                });
            ui.add_space(6.0);
        }
    });
    action
}

fn show_json_preview(editor: &ProfileEditor, ui: &mut egui::Ui) {
    ui.heading("JSON preview");
    ui.weak("Read-only preview of the complete profile document, including preserved fields.");
    ui.separator();
    let json = serde_json::to_string_pretty(&editor.document)
        .unwrap_or_else(|error| format!("Could not render JSON: {error}"));
    egui::ScrollArea::both().show(ui, |ui| {
        ui.add(
            egui::Label::new(egui::RichText::new(json).monospace())
                .selectable(true)
                .wrap(),
        );
    });
}

fn collect_ffb_settings(document: &serde_json::Value) -> Vec<JsonSetting> {
    fn visit(value: &serde_json::Value, path: &mut Vec<String>, out: &mut Vec<JsonSetting>) {
        let Some(object) = value.as_object() else {
            return;
        };
        for (key, child) in object {
            if key == "Input" || key == "Alternative Input" {
                continue;
            }
            path.push(key.clone());
            if (child.is_number() || child.is_boolean()) && is_ffb_setting_name(key) {
                out.push(JsonSetting {
                    path: path.clone(),
                    label: key.clone(),
                });
            } else if child.is_object() {
                visit(child, path, out);
            }
            path.pop();
        }
    }

    let mut settings = Vec::new();
    visit(document, &mut Vec::new(), &mut settings);
    settings.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    settings
}

fn is_ffb_setting_name(name: &str) -> bool {
    let name = name.to_lowercase();
    [
        "force feedback",
        "ffb",
        "steering torque",
        "minimum torque",
        "collision strength",
        "smoothing",
        "constant steering",
        "vendor ffb",
        "haptic",
        "vibrotactile",
        "rumble",
        "jolt",
        "steering resistance",
        "steering spring",
    ]
    .iter()
    .any(|term| name.contains(term))
}

fn json_value_mut<'a>(
    value: &'a mut serde_json::Value,
    path: &[String],
) -> Option<&'a mut serde_json::Value> {
    let mut current = value;
    for segment in path {
        current = current.as_object_mut()?.get_mut(segment)?;
    }
    Some(current)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_editable_ffb_values_without_treating_bindings_as_settings() {
        let mut document = serde_json::json!({
            "Devices": {
                "Wheel": {
                    "FFB Strength": 0.7,
                    "Invert FFB": false,
                    "Unrelated calibration": 12
                }
            },
            "Input": {
                "Reset FFB": {"device": "Wheel", "id": 40}
            }
        });

        let settings = collect_ffb_settings(&document);
        assert_eq!(settings.len(), 2);
        let strength = settings
            .iter()
            .find(|setting| setting.label == "FFB Strength")
            .unwrap();
        *json_value_mut(&mut document, &strength.path).unwrap() = serde_json::json!(0.8);
        assert_eq!(document["Devices"]["Wheel"]["FFB Strength"], 0.8);
        assert_eq!(document["Devices"]["Wheel"]["Unrelated calibration"], 12);
    }
}
