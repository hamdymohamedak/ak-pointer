//! The actions directory. [`ActionStore`] is the only way the UI reads and writes files.

mod codec;

pub use codec::{format_action, parse_action_text, sanitize_action_name};

use std::fs;
use std::path::{Path, PathBuf};

use crate::model::Action;

use codec::{list_action_files, load_action, save_action};

pub struct CatalogEntry {
    pub name: String,
    pub path: PathBuf,
    pub action: Option<Action>,
}

#[derive(Clone)]
pub struct ActionStore {
    dir: PathBuf,
}

impl ActionStore {
    pub fn open(dir: impl Into<PathBuf>) -> std::io::Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    pub fn directory(&self) -> &Path {
        &self.dir
    }

    pub fn catalog(&self) -> Vec<CatalogEntry> {
        list_action_files(&self.dir)
            .into_iter()
            .map(|path| {
                let name = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("")
                    .to_string();
                let action = load_action(&path);
                CatalogEntry { name, path, action }
            })
            .collect()
    }

    pub fn path_for(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.txt"))
    }

    pub fn load(&self, name: &str) -> Option<Action> {
        load_action(&self.path_for(name))
    }

    pub fn save(&self, name: &str, action: &Action) -> bool {
        save_action(&self.path_for(name), action)
    }

    pub fn delete(&self, name: &str) -> bool {
        fs::remove_file(self.path_for(name)).is_ok()
    }

    pub fn exists(&self, name: &str) -> bool {
        self.path_for(name).is_file()
    }
}
