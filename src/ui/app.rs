use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;

use super::editor::{Editor, EditorResult};
use crate::engine::{self as recorder, play_action, AllowedWindow, PlayStatus, RecordStatus};
use crate::model::{Action, EventType};
use crate::platform::{self as window, AppChoice};
use crate::store::{sanitize_action_name, ActionStore};

enum Note {
    Status(String),
    Idle,
}

struct SavedAction {
    name: String,
    summary: String,
    window: String,
    readable: bool,
}

pub struct RecorderApp {
    store: ActionStore,
    actions: Vec<SavedAction>,
    name: String,
    queue: Vec<String>,
    apps: Vec<AppChoice>,
    picked: Vec<(String, String)>,
    last_apps: Instant,
    status: String,
    busy: bool,
    replace_name: Option<String>,
    delete_name: Option<String>,
    editor: Option<Editor>,
    notes: Option<Receiver<Note>>,
    cancel: Arc<AtomicBool>,
}

impl RecorderApp {
    fn new(store: ActionStore) -> Self {
        let mut app = Self {
            store,
            actions: Vec::new(),
            name: String::new(),
            queue: Vec::new(),
            apps: Vec::new(),
            picked: Vec::new(),
            last_apps: Instant::now(),
            status: "Choose the app, record an action, then play the list in order.".into(),
            busy: false,
            replace_name: None,
            delete_name: None,
            editor: None,
            notes: None,
            cancel: Arc::new(AtomicBool::new(false)),
        };
        app.reload();
        app.refresh_apps();
        app
    }

    fn refresh_apps(&mut self) {
        self.apps = window::list_app_choices();
        self.last_apps = Instant::now();
    }

    fn reload(&mut self) {
        self.actions = self
            .store
            .catalog()
            .into_iter()
            .map(|entry| {
                let name = entry.name;
                match entry.action {
                    Some(action) => {
                        let mut window = action.window_owner.clone();
                        if !action.window_title.is_empty() {
                            if !window.is_empty() {
                                window.push_str(" / ");
                            }
                            window.push_str(&action.window_title);
                        }
                        SavedAction {
                            name,
                            summary: tally(&action),
                            window,
                            readable: true,
                        }
                    }
                    None => SavedAction {
                        name,
                        summary: String::new(),
                        window: "unreadable file".into(),
                        readable: false,
                    },
                }
            })
            .collect();
    }

    fn poll(&mut self) {
        let Some(rx) = self.notes.take() else {
            return;
        };
        let mut idle = false;
        while let Ok(note) = rx.try_recv() {
            match note {
                Note::Status(text) => self.status = text,
                Note::Idle => idle = true,
            }
        }
        self.notes = Some(rx);
        if idle {
            self.busy = false;
            self.reload();
            self.refresh_apps();
        }
    }

    fn stop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        recorder::stop_recording();
        self.status = "Stopping…".into();
    }

    fn spawn(
        &mut self,
        status: String,
        job: impl FnOnce(Sender<Note>, Arc<AtomicBool>) + Send + 'static,
    ) {
        self.cancel.store(false, Ordering::SeqCst);
        self.busy = true;
        self.status = status;
        let (tx, rx) = mpsc::channel();
        self.notes = Some(rx);
        let cancel = Arc::clone(&self.cancel);
        thread::spawn(move || {
            let _finish = IdleOnDrop(tx.clone());
            job(tx, cancel);
        });
    }

    fn record(&mut self) {
        if self.busy {
            return;
        }
        let allowed: Vec<AllowedWindow> = self
            .picked
            .iter()
            .map(|(owner, title)| AllowedWindow {
                owner: owner.clone(),
                title: title.clone(),
            })
            .collect();
        if allowed.is_empty() {
            self.status =
                "Choose at least one app on the left. For example the iPhone simulator.".into();
            return;
        }
        let name = sanitize_action_name(&self.name);
        if name.is_empty() {
            self.status = "Enter a name for the action.".into();
            return;
        }
        if self.store.exists(&name) && self.replace_name.as_deref() != Some(name.as_str()) {
            self.replace_name = Some(name.clone());
            self.status = format!("{name} already exists. Press Record again to replace it.");
            return;
        }
        self.replace_name = None;
        self.name = name.clone();
        let chosen = self
            .picked
            .iter()
            .map(|(owner, title)| {
                if title.is_empty() {
                    owner.clone()
                } else {
                    format!("{owner} — {title}")
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        recorder::prepare_recording();
        let mut status = format!(
            "Recording on {chosen}. Clicks, drags, moves, and keys are tracked. Other apps are ignored. Press Stop or F8."
        );
        if !window::keyboard_capture_ready() {
            status.push_str(" Keyboard needs Input Monitoring for Cursor.");
        }
        let store = self.store.clone();
        self.spawn(status, move |tx, _cancel| {
            let result = recorder::record_action(allowed);
            let message = match result.status {
                RecordStatus::NoAccessibility => {
                    "Accessibility is off. Enable Cursor in System Settings, then try again.".into()
                }
                RecordStatus::NoWindow => {
                    "The selected app has no window. Open it, then record again.".into()
                }
                RecordStatus::Ok if result.action.events.is_empty() => {
                    let message = "Nothing was recorded inside the selected window, so it was not saved. Click the simulator screen, then press Stop.".to_string();
                    println!("{message}");
                    message
                }
                RecordStatus::Ok => {
                    let path = store.path_for(&name);
                    if store.save(&name, &result.action) {
                        let message = format!("Saved {}. {}", path.display(), tally(&result.action));
                        println!("{message}");
                        message
                    } else {
                        let message = format!("Could not save {}.", path.display());
                        println!("{message}");
                        message
                    }
                }
            };
            let _ = tx.send(Note::Status(message));
        });
    }

    fn play_names(&mut self, names: Vec<String>) {
        if self.busy || names.is_empty() {
            return;
        }
        let store = self.store.clone();
        let count = names.len();
        self.spawn(
            format!(
                "Playing {count} action{}. The app will be brought to the front. Stop cancels.",
                if count == 1 { "" } else { "s" }
            ),
            move |tx, cancel| {
                for (index, name) in names.iter().enumerate() {
                    if cancel.load(Ordering::Relaxed) {
                        let _ = tx.send(Note::Status("Stopped.".into()));
                        return;
                    }
                    let Some(action) = store.load(name) else {
                        let _ = tx.send(Note::Status(format!("{name} could not be read.")));
                        return;
                    };
                    let _ = tx.send(Note::Status(format!(
                        "Playing {name} ({}/{}). Bringing {} to the front.",
                        index + 1,
                        names.len(),
                        action.window_owner
                    )));
                    let countdown = if index == 0 { 3 } else { 0 };
                    match play_action(&action, &cancel, countdown) {
                        PlayStatus::Done => {}
                        PlayStatus::Cancelled => {
                            let _ = tx.send(Note::Status("Stopped.".into()));
                            return;
                        }
                        PlayStatus::WindowNotFound => {
                            let _ = tx.send(Note::Status(format!(
                                "Could not find the emulator window for {name}."
                            )));
                            return;
                        }
                        PlayStatus::NoAccessibility => {
                            let _ = tx.send(Note::Status(
                                "Accessibility is off. Enable Cursor in System Settings.".into(),
                            ));
                            return;
                        }
                    }
                }
                let _ = tx.send(Note::Status(format!(
                    "Finished {count} action{}.",
                    if count == 1 { "" } else { "s" }
                )));
            },
        );
    }

    fn delete_action(&mut self, name: &str) {
        if !self.store.delete(name) {
            self.status = format!("Could not delete {name}.");
            return;
        }
        self.queue.retain(|queued| queued != name);
        self.delete_name = None;
        self.status = format!("Deleted {name}.");
        self.reload();
    }

    fn open_editor(&mut self, name: &str) {
        let path = self.store.path_for(name);
        let Some(action) = self.store.load(name) else {
            self.status = format!("{name} could not be read.");
            return;
        };
        self.editor = Some(Editor::new(
            name.to_string(),
            path,
            action,
            self.store.clone(),
        ));
        self.status = format!("Editing {name}. Correct the typed text, then save.");
    }
}

impl eframe::App for RecorderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        if self.busy {
            ctx.request_repaint_after(Duration::from_millis(100));
        } else if self.last_apps.elapsed() >= Duration::from_secs(2) {
            self.refresh_apps();
            ctx.request_repaint_after(Duration::from_secs(2));
        }

        egui::TopBottomPanel::top("record").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading("ak-pointer");
                ui.weak("1 choose the app   ·   2 record   ·   3 play the list");
            });
            if !window::keyboard_capture_ready() {
                ui.colored_label(
                    egui::Color32::from_rgb(210, 150, 50),
                    "Keyboard is not tracked yet. System Settings → Privacy & Security → Input Monitoring → enable Cursor, then run this again.",
                );
            }
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.name)
                        .hint_text("Action name")
                        .desired_width(240.0),
                );
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Record"))
                    .clicked()
                {
                    self.record();
                }
                if ui
                    .add_enabled(self.busy, egui::Button::new("Stop"))
                    .clicked()
                {
                    self.stop();
                }
            });
            ui.label(&self.status);
            ui.add_space(4.0);
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(&self.status);
            ui.add_space(4.0);
        });

        egui::SidePanel::left("apps")
            .resizable(true)
            .default_width(320.0)
            .show(ctx, |ui| {
                self.apps_ui(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let result = if let Some(editor) = &mut self.editor {
                editor.ui(ui)
            } else {
                self.queue_ui(ui);
                ui.add_space(8.0);
                ui.separator();
                self.library_ui(ui);
                EditorResult::Stay
            };
            match result {
                EditorResult::Back => {
                    self.editor = None;
                    self.reload();
                }
                EditorResult::Changed => self.reload(),
                EditorResult::Stay => {}
            }
        });
    }
}

impl RecorderApp {
    fn apps_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Apps");
        ui.label(
            "Check every app this recording may touch. A background app is brought to the front.",
        );
        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                self.refresh_apps();
            }
            ui.weak(format!("{} selected", self.picked.len()));
        });
        ui.add_space(4.0);
        if self.apps.is_empty() {
            ui.weak("No app windows found.");
            return;
        }
        let picked = self.picked.clone();
        let mut toggle = None;
        let mut last_owner = String::new();
        egui::ScrollArea::vertical().id_salt("apps").show(ui, |ui| {
            for app in &self.apps {
                if app.owner != last_owner {
                    ui.add_space(6.0);
                    ui.strong(&app.owner);
                    last_owner = app.owner.clone();
                }
                let title = if app.title.is_empty() {
                    format!("{:.0} × {:.0}", app.w, app.h)
                } else {
                    format!("{}    {:.0} × {:.0}", app.title, app.w, app.h)
                };
                let mut on = picked
                    .iter()
                    .any(|(owner, title)| owner == &app.owner && title == &app.title);
                if ui.checkbox(&mut on, title).changed() {
                    toggle = Some((app.owner.clone(), app.title.clone(), on));
                }
                if !app.visible {
                    ui.weak("        in the background");
                }
            }
        });
        if let Some((owner, title, on)) = toggle {
            if on {
                self.picked.push((owner, title));
            } else {
                self.picked.retain(|(picked_owner, picked_title)| {
                    !(picked_owner == &owner && picked_title == &title)
                });
            }
        }
    }

    fn library_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Saved");
        ui.add_space(4.0);
        if self.actions.is_empty() {
            ui.weak("No saved actions yet.");
            return;
        }

        let mut command: Option<RowCommand> = None;
        let busy = self.busy;
        let delete_name = self.delete_name.clone();
        egui::ScrollArea::vertical()
            .id_salt("library")
            .show(ui, |ui| {
                for action in &self.actions {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}    {}", action.name, action.window));
                        ui.weak(&action.summary);
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(action.readable && !busy, egui::Button::new("Edit"))
                            .clicked()
                        {
                            command = Some(RowCommand::Edit(action.name.clone()));
                        }
                        if ui
                            .add_enabled(action.readable && !busy, egui::Button::new("Play"))
                            .clicked()
                        {
                            command = Some(RowCommand::Play(action.name.clone()));
                        }
                        if ui
                            .add_enabled(action.readable, egui::Button::new("Add to queue"))
                            .clicked()
                        {
                            command = Some(RowCommand::Queue(action.name.clone()));
                        }
                        let confirming = delete_name.as_deref() == Some(action.name.as_str());
                        let label = if confirming {
                            "Confirm delete"
                        } else {
                            "Delete"
                        };
                        if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                            command = Some(if confirming {
                                RowCommand::Delete(action.name.clone())
                            } else {
                                RowCommand::AskDelete(action.name.clone())
                            });
                        }
                    });
                    ui.separator();
                }
            });

        match command {
            Some(RowCommand::Edit(name)) => self.open_editor(&name),
            Some(RowCommand::Play(name)) => self.play_names(vec![name]),
            Some(RowCommand::Queue(name)) => {
                self.queue.push(name.clone());
                self.status = format!("Added {name} to the queue.");
            }
            Some(RowCommand::AskDelete(name)) => self.delete_name = Some(name),
            Some(RowCommand::Delete(name)) => self.delete_action(&name),
            None => {}
        }
    }

    fn queue_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Play in this order");
        ui.label("Add saved actions below. They run from top to bottom, and each app is brought forward.");
        ui.add_space(6.0);
        let count = self.queue.len();
        if ui
            .add_enabled(
                !self.busy && count > 0,
                egui::Button::new(format!("Play {count} in order"))
                    .min_size(egui::vec2(180.0, 32.0)),
            )
            .clicked()
        {
            self.play_names(self.queue.clone());
        }
        if ui
            .add_enabled(!self.queue.is_empty(), egui::Button::new("Clear queue"))
            .clicked()
        {
            self.queue.clear();
        }
        ui.add_space(8.0);
        if self.queue.is_empty() {
            ui.weak("Add actions from the list.");
            return;
        }

        let mut command: Option<QueueCommand> = None;
        egui::ScrollArea::vertical()
            .id_salt("queue")
            .show(ui, |ui| {
                for (index, name) in self.queue.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}. {name}", index + 1));
                        if ui.add_enabled(index > 0, egui::Button::new("Up")).clicked() {
                            command = Some(QueueCommand::Up(index));
                        }
                        if ui
                            .add_enabled(index + 1 < count, egui::Button::new("Down"))
                            .clicked()
                        {
                            command = Some(QueueCommand::Down(index));
                        }
                        if ui.button("Remove").clicked() {
                            command = Some(QueueCommand::Remove(index));
                        }
                    });
                }
            });
        match command {
            Some(QueueCommand::Up(index)) => self.queue.swap(index, index - 1),
            Some(QueueCommand::Down(index)) => self.queue.swap(index, index + 1),
            Some(QueueCommand::Remove(index)) => {
                self.queue.remove(index);
            }
            None => {}
        }
    }
}

enum RowCommand {
    Edit(String),
    Play(String),
    Queue(String),
    AskDelete(String),
    Delete(String),
}

enum QueueCommand {
    Up(usize),
    Down(usize),
    Remove(usize),
}

fn tally(action: &Action) -> String {
    let mut clicks = 0;
    let mut drags = 0;
    let mut moves = 0;
    let mut keys = 0;
    let mut scrolls = 0;
    for event in &action.events {
        match event.kind {
            EventType::Down => clicks += 1,
            EventType::Drag => drags += 1,
            EventType::Move => moves += 1,
            EventType::Key if event.key_down => keys += 1,
            EventType::Scroll => scrolls += 1,
            _ => {}
        }
    }
    format!("{clicks} clicks · {drags} drags · {moves} moves · {keys} keys · {scrolls} scrolls")
}

struct IdleOnDrop(Sender<Note>);

impl Drop for IdleOnDrop {
    fn drop(&mut self) {
        let _ = self.0.send(Note::Idle);
    }
}

pub fn run() -> eframe::Result<()> {
    let actions_dir = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("actions");
    let store = match ActionStore::open(&actions_dir) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("Could not create {}: {error}", actions_dir.display());
            std::process::exit(1);
        }
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1080.0, 700.0])
            .with_min_inner_size([860.0, 540.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ak-pointer",
        options,
        Box::new(move |_cc| Ok(Box::new(RecorderApp::new(store)))),
    )
}
