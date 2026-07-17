use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::recipes::recipe::RecipeError;

/// A single entry inside a tag's `values` array. Almost always a bare id string
/// (e.g. `"minecraft:acacia_log"` or a nested tag `"#minecraft:chest_boats"`),
/// but the data format also permits an object form `{ "id": "...", ... }`.
#[derive(Deserialize)]
#[serde(untagged)]
enum TagEntry {
    Id(String),
    Object { id: String },
}

impl TagEntry {
    fn id(&self) -> &str {
        match self {
            TagEntry::Id(id) => id,
            TagEntry::Object { id } => id,
        }
    }
}

#[derive(Deserialize)]
struct TagFile {
    #[serde(default)]
    values: Vec<TagEntry>,
}

/// Minecraft item tags, loaded from `data/minecraft/tags/item`. A tag id such as
/// `minecraft:planks` maps to a list of member ids, any of which may itself be a
/// nested tag reference (`#minecraft:chest_boats`) that must be expanded.
#[derive(Default)]
pub struct Tags {
    /// tag id (with namespace, e.g. `minecraft:acacia_logs`) -> raw member tokens.
    tags: HashMap<String, Vec<String>>,
}

impl Tags {
    /// Registers one tag: a full tag id (e.g. `minecraft:planks`) and its member
    /// tokens, each an item id or a nested `#tag` reference. Re-registering a tag
    /// replaces its previous members.
    pub fn add(&mut self, tag: String, members: Vec<String>) {
        self.tags.insert(tag, members);
    }

    /// Loads every `*.json` tag file under `dir`, keyed by `minecraft:<relative/path>`
    /// so nested tag ids (which may contain `/`) resolve to the correct file.
    pub fn load_from_filesystem(dir: impl AsRef<Path>) -> Result<Self, RecipeError> {
        let dir = dir.as_ref();
        let mut tags = HashMap::new();
        Self::load_dir(dir, dir, &mut tags)?;
        Ok(Self { tags })
    }

    fn load_dir(
        root: &Path,
        current: &Path,
        tags: &mut HashMap<String, Vec<String>>,
    ) -> Result<(), RecipeError> {
        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                Self::load_dir(root, &path, tags)?;
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/");
            let content = std::fs::read_to_string(&path)?;
            let file: TagFile = serde_json::from_str(&content)?;
            let values = file.values.iter().map(|v| v.id().to_string()).collect();
            tags.insert(format!("minecraft:{relative}"), values);
        }
        Ok(())
    }

    /// Resolves an ingredient token to the flat set of concrete item ids it can be
    /// satisfied by. A `#`-prefixed token is a tag and is expanded recursively
    /// (nested tags included); any other token is treated as a literal item id.
    pub fn resolve(&self, token: &str) -> HashSet<String> {
        let mut out = HashSet::new();
        let mut visiting = HashSet::new();
        self.resolve_into(token, &mut out, &mut visiting);
        out
    }

    fn resolve_into(&self, token: &str, out: &mut HashSet<String>, visiting: &mut HashSet<String>) {
        if let Some(tag) = token.strip_prefix('#') {
            if !visiting.insert(tag.to_string()) {
                return; // already expanding this tag -> break the cycle
            }
            if let Some(values) = self.tags.get(tag) {
                for value in values {
                    self.resolve_into(value, out, visiting);
                }
            }
            visiting.remove(tag);
        } else {
            out.insert(token.to_string());
        }
    }
}
