use std::path::PathBuf;

use eframe::egui;

use crate::model::{apply_text_run, describe_event, text_runs, Action, EventType, TextRun};
use crate::store::{format_action, parse_action_text, ActionStore};

pub(crate) enum EditorResult {
    Stay,
    Back,
    Changed,
}

struct Draft {
    start: usize,
    end: usize,
    original: String,
    edited: String,
}

pub(crate) struct Editor {
    name: String,
    path: PathBuf,
    store: ActionStore,
    action: Action,
    owner: String,
    title: String,
    drafts: Vec<Draft>,
    show_moves: bool,
    raw_mode: bool,
    raw: String,
    message: String,
}

impl Editor {
    pub(crate) fn new(name: String, path: PathBuf, action: Action, store: ActionStore) -> Self {
        let owner = action.window_owner.clone();
        let title = action.window_title.clone();
        let drafts = drafts_from(&action);
        Self {
            name,
            path,
            store,
            action,
            owner,
            title,
            drafts,
            show_moves: false,
            raw_mode: false,
            raw: String::new(),
            message: String::new(),
        }
    }

    fn sync_header(&mut self) {
        self.action.window_owner = self.owner.clone();
        self.action.window_title = self.title.clone();
    }

    pub(crate) fn ui(&mut self, ui: &mut egui::Ui) -> EditorResult {
        let mut result = EditorResult::Stay;
        ui.horizontal(|ui| {
            if ui.button("Back").clicked() {
                result = EditorResult::Back;
            }
            ui.heading(&self.name);
            ui.weak(self.path.display().to_string());
        });
        if matches!(result, EditorResult::Back) {
            return result;
        }
        ui.label(format!(
            "Recorded window size {:.0} x {:.0}",
            self.action.window_w, self.action.window_h
        ));
        ui.horizontal(|ui| {
            ui.label("Window owner");
            ui.add(egui::TextEdit::singleline(&mut self.owner).desired_width(220.0));
            ui.label("Title");
            ui.add(egui::TextEdit::singleline(&mut self.title).desired_width(220.0));
        });
        ui.weak("Playback looks for this owner and title.");

        let was_raw = self.raw_mode;
        ui.checkbox(&mut self.raw_mode, "Edit the action file as text");
        if self.raw_mode && !was_raw {
            self.sync_header();
            self.raw = format_action(&self.action);
        }

        if self.raw_mode {
            if ui.button("Save file").clicked() {
                result = self.save_raw();
            }
            egui::ScrollArea::vertical().id_salt("raw").show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.raw)
                        .desired_width(f32::INFINITY)
                        .desired_rows(18)
                        .code_editor(),
                );
            });
        } else {
            ui.add_space(6.0);
            ui.label("Typed text. A wrong word can be corrected here. Backspaces from the recording are already applied.");
            if self.drafts.is_empty() {
                ui.weak("This action has no plain typing. Delete an event below, or edit the file text.");
            }
            let dirty = self
                .drafts
                .iter()
                .any(|draft| draft.edited != draft.original);
            if ui
                .add_enabled(dirty, egui::Button::new("Save typed text"))
                .clicked()
            {
                result = self.save_text();
            }
            egui::ScrollArea::vertical()
                .id_salt("drafts")
                .max_height(180.0)
                .show(ui, |ui| {
                    for (index, draft) in self.drafts.iter_mut().enumerate() {
                        ui.label(format!("Typing {}", index + 1));
                        ui.add(
                            egui::TextEdit::multiline(&mut draft.edited)
                                .desired_width(f32::INFINITY)
                                .desired_rows(2),
                        );
                    }
                });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(format!("{} events", self.action.events.len()));
                ui.checkbox(&mut self.show_moves, "Show mouse moves");
            });
            let dirty_now = self
                .drafts
                .iter()
                .any(|draft| draft.edited != draft.original);
            let mut remove_at = None;
            egui::ScrollArea::vertical()
                .id_salt("events")
                .show(ui, |ui| {
                    for (index, event) in self.action.events.iter().enumerate() {
                        if !self.show_moves && event.kind == EventType::Move {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            ui.monospace(format!(
                                "{:>6} ms   {}",
                                event.t_ms,
                                describe_event(event)
                            ));
                            if ui.button("Delete").clicked() {
                                remove_at = Some(index);
                            }
                        });
                    }
                });
            if let Some(index) = remove_at {
                if dirty_now {
                    self.message = "Save the typed text before deleting events.".into();
                } else {
                    self.action.events.remove(index);
                    result = self.write_action("Deleted the event.");
                }
            }
        }

        if !self.message.is_empty() {
            ui.add_space(4.0);
            ui.label(&self.message);
        }
        result
    }

    fn save_text(&mut self) -> EditorResult {
        self.sync_header();
        let mut events = self.action.events.clone();
        let mut pending: Vec<(usize, usize, String)> = self
            .drafts
            .iter()
            .filter(|draft| draft.edited != draft.original)
            .map(|draft| (draft.start, draft.end, draft.edited.clone()))
            .collect();
        pending.sort_by(|left, right| right.0.cmp(&left.0));
        for (start, end, text) in pending {
            if let Err(error) = apply_text_run(&mut events, start, end, &text) {
                self.message = error;
                return EditorResult::Stay;
            }
        }
        self.action.events = events;
        self.write_action("Saved the typed text.")
    }

    fn save_raw(&mut self) -> EditorResult {
        let Some(action) = parse_action_text(&self.raw) else {
            self.message = "Could not read that file. Check the lines and try again.".into();
            return EditorResult::Stay;
        };
        self.action = action;
        self.owner = self.action.window_owner.clone();
        self.title = self.action.window_title.clone();
        self.drafts = drafts_from(&self.action);
        self.write_action("Saved the action file.")
    }

    fn write_action(&mut self, message: &str) -> EditorResult {
        self.sync_header();
        if !self.store.save(&self.name, &self.action) {
            self.message = "Could not save the action file.".into();
            return EditorResult::Stay;
        }
        self.drafts = drafts_from(&self.action);
        self.raw = format_action(&self.action);
        self.message = message.into();
        EditorResult::Changed
    }
}

fn drafts_from(action: &Action) -> Vec<Draft> {
    text_runs(&action.events)
        .into_iter()
        .map(|run: TextRun| Draft {
            start: run.start,
            end: run.end,
            edited: run.text.clone(),
            original: run.text,
        })
        .collect()
}
