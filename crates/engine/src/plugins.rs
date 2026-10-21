//! Installed plugins (docs/plugins.md): what the desktop app, the command line and the control
//! channel know about the folders under `{data}/plugins`.
//!
//! A plugin is a folder `<plugins dir>/<id>/plugin.json`. This module reads and checks those
//! manifests and works out, from the user's choices (turned off, permissions allowed), which
//! plugins are on and what they may use. It never loads or runs anything from a plugin folder.
//!
//! Permissions are opt-in twice: the manifest asks for them, and the user allows them for that
//! plugin. A plugin that asks for a permission the user has not allowed is held until they do
//! ([`State::NeedsApproval`]); one that asks for a permission this version does not know is not a
//! plugin at all ([`Skipped`]), so nothing ever runs with less than it expects.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How many plugin ids the preferences may name (turned off, or with permissions allowed).
pub const MAX_CHOICES: usize = 256;
/// How many folders a scan looks at; the rest are reported as skipped.
pub const MAX_FOLDERS: usize = 256;
/// `plugin.json` larger than this is refused.
pub const MAX_MANIFEST_BYTES: u64 = 16 * 1024;
/// The manifest file in a plugin's folder.
pub const MANIFEST: &str = "plugin.json";
/// Moves the plugins folder (tests, portable installs, a package that ships its plugins).
pub const DIR_ENV: &str = "SOLIDCRAFT_PLUGINS_DIR";

/// Something a plugin may ask to use. The user allows each one per plugin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// The control channel (docs/control-protocol.md) is opened at start on a free loopback port,
    /// and its port and token are written where the plugin's program finds them
    /// ([`crate::control_auth::plugin_endpoint_path`]).
    Control,
}

impl Permission {
    /// Every permission this version knows.
    pub const ALL: [Permission; 1] = [Permission::Control];

    /// The id a manifest and the preferences use.
    pub fn id(self) -> &'static str {
        match self {
            Permission::Control => "control",
        }
    }

    pub fn from_id(id: &str) -> Option<Permission> {
        Permission::ALL.into_iter().find(|p| p.id() == id)
    }
}

/// One installed plugin, as its manifest describes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    /// What it asks for: sorted, no repeats.
    pub permissions: Vec<Permission>,
}

impl Plugin {
    pub fn wants(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }
}

/// A folder under the plugins directory that is not a plugin, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Skipped {
    pub folder: String,
    pub reason: String,
}

/// `plugin.json`. Any member not listed here makes the folder a skipped one, so a manifest
/// written for a newer SolidCraft says so instead of loading with part of it ignored.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    id: String,
    name: String,
    version: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    permissions: Vec<String>,
}

/// The plugins directory: [`DIR_ENV`] when set, else `plugins` in the per-user data folder
/// ([`crate::recovery::data_dir`]). `None` when no folder can be named (the browser build).
pub fn plugins_dir() -> Option<PathBuf> {
    if cfg!(target_arch = "wasm32") {
        return None;
    }
    match std::env::var_os(DIR_ENV).filter(|d| !d.is_empty()) {
        Some(d) => Some(PathBuf::from(d)),
        None => crate::recovery::data_dir().map(|d| d.join("plugins")),
    }
}

/// An id a folder and a manifest may share: 1–64 characters, lowercase letters, digits and
/// hyphens, not starting or ending with a hyphen.
pub fn valid_id(id: &str) -> bool {
    let b = id.as_bytes();
    (1..=64).contains(&b.len())
        && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
        && !id.starts_with('-')
        && !id.ends_with('-')
}

fn plain(text: &str, max: usize) -> bool {
    !text.is_empty() && text.chars().count() <= max && text.chars().all(|c| !c.is_control())
}

fn check(m: Manifest, folder: &str) -> Result<Plugin, String> {
    if !valid_id(&m.id) {
        return Err("the id must be 1–64 lowercase letters, digits and hyphens".into());
    }
    if m.id != folder {
        return Err(format!("plugin.json id `{}` does not match the folder `{folder}`", m.id));
    }
    let name = m.name.trim();
    if !plain(name, 80) {
        return Err("the name must be 1–80 characters with no line breaks".into());
    }
    let version = m.version.trim();
    if !plain(version, 32) || !version.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+')) {
        return Err("the version must be 1–32 letters, digits, dots, hyphens or plus signs".into());
    }
    let description = m.description.trim();
    if description.chars().count() > 240 || description.chars().any(|c| c.is_control()) {
        return Err("the description must be at most 240 characters with no line breaks".into());
    }
    if m.permissions.len() > 32 {
        return Err("at most 32 permissions".into());
    }
    let mut permissions = Vec::new();
    for p in &m.permissions {
        let Some(permission) = Permission::from_id(p) else {
            let known = Permission::ALL.map(Permission::id).join(", ");
            let shown: String = p.chars().take(32).collect();
            return Err(format!("permission `{shown}` is not one this SolidCraft knows ({known}); a newer SolidCraft may"));
        };
        if !permissions.contains(&permission) {
            permissions.push(permission);
        }
    }
    permissions.sort();
    Ok(Plugin { id: m.id, name: name.to_string(), version: version.to_string(), description: description.to_string(), permissions })
}

/// The plugin in folder `dir`, checked as a scan checks it: its `plugin.json` must name the
/// folder's own name as its `id`. For plugin authors (`solidcraft-cli plugins check`) as for the
/// scan.
pub fn read(dir: &Path) -> Result<Plugin, String> {
    let folder = dir.file_name().and_then(|f| f.to_str()).ok_or("the folder name is not text")?;
    let path = dir.join(MANIFEST);
    let meta = std::fs::metadata(&path).map_err(|_| format!("no {MANIFEST}"))?;
    if !meta.is_file() || meta.len() > MAX_MANIFEST_BYTES {
        return Err(format!("{MANIFEST} is not a file or is larger than 16 KiB"));
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{MANIFEST} could not be read ({e})"))?;
    let manifest: Manifest = serde_json::from_str(&text).map_err(|e| {
        let hint = if e.to_string().contains("unknown field") { "; it may be for a newer SolidCraft" } else { "" };
        format!("{MANIFEST} is not a plugin manifest ({e}{hint})")
    })?;
    check(manifest, folder)
}

/// The plugins in `dir`, sorted by name, and the folders that were not plugins, sorted by
/// folder. A missing directory is an empty list. Folders whose names start with `.` are ignored.
pub fn scan(dir: &Path) -> (Vec<Plugin>, Vec<Skipped>) {
    let mut plugins = Vec::new();
    let mut skipped = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (plugins, skipped);
    };
    let mut seen = 0usize;
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        if !kind.is_dir() {
            continue;
        }
        let Ok(folder) = entry.file_name().into_string() else {
            skipped.push(Skipped { folder: String::new(), reason: "the folder name is not text".into() });
            continue;
        };
        if folder.starts_with('.') {
            continue;
        }
        seen += 1;
        if seen > MAX_FOLDERS {
            skipped.push(Skipped { folder, reason: format!("more than {MAX_FOLDERS} plugin folders") });
            continue;
        }
        match read(&entry.path()) {
            Ok(plugin) => plugins.push(plugin),
            Err(reason) => skipped.push(Skipped { folder, reason }),
        }
    }
    plugins.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    skipped.sort_by(|a, b| a.folder.cmp(&b.folder));
    skipped.truncate(MAX_FOLDERS);
    (plugins, skipped)
}

/// What the user decided about plugins (kept in the desktop app's preferences).
#[derive(Clone, Copy, Debug)]
pub struct Choices<'a> {
    /// Ids turned off. An installed plugin whose id is absent is on.
    pub disabled: &'a BTreeSet<String>,
    /// Per id, the permission ids the user allowed.
    pub allowed: &'a BTreeMap<String, BTreeSet<String>>,
}

/// Whether a plugin is in use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    On,
    /// Turned off by the user.
    Off,
    /// On, but it asks for permissions the user has not allowed (yet), so it is not used.
    NeedsApproval,
}

impl Choices<'_> {
    /// The permissions `plugin` asks for that the user has not allowed.
    pub fn missing(&self, plugin: &Plugin) -> Vec<Permission> {
        let allowed = self.allowed.get(&plugin.id);
        plugin.permissions.iter().copied().filter(|p| !allowed.is_some_and(|a| a.contains(p.id()))).collect()
    }

    pub fn state(&self, plugin: &Plugin) -> State {
        if self.disabled.contains(&plugin.id) {
            State::Off
        } else if self.missing(plugin).is_empty() {
            State::On
        } else {
            State::NeedsApproval
        }
    }

    /// The ids of the plugins that are on and use `permission`.
    pub fn using(&self, plugins: &[Plugin], permission: Permission) -> Vec<String> {
        plugins.iter().filter(|p| p.wants(permission) && self.state(p) == State::On).map(|p| p.id.clone()).collect()
    }
}

/// Whether saved or requested choices are within bounds: at most [`MAX_CHOICES`] ids each, every
/// id a [`valid_id`], every allowed permission one this version knows. The message names what
/// is wrong.
pub fn check_choices(choices: Choices<'_>) -> Result<(), String> {
    if choices.disabled.len() > MAX_CHOICES || choices.disabled.iter().any(|id| !valid_id(id)) {
        return Err(format!("`disabled_plugins` must be at most {MAX_CHOICES} ids of lowercase letters, digits and hyphens"));
    }
    let known = |p: &String| Permission::from_id(p).is_some();
    if choices.allowed.len() > MAX_CHOICES || choices.allowed.iter().any(|(id, ps)| !valid_id(id) || !ps.iter().all(known)) {
        let ids = Permission::ALL.map(Permission::id).join(", ");
        return Err(format!("`plugin_permissions` must map at most {MAX_CHOICES} plugin ids to lists of permissions ({ids})"));
    }
    Ok(())
}

/// Drop what [`check_choices`] would refuse, for a hand-edited preferences file.
pub fn limit_choices(disabled: &mut BTreeSet<String>, allowed: &mut BTreeMap<String, BTreeSet<String>>) {
    disabled.retain(|id| valid_id(id));
    while disabled.len() > MAX_CHOICES {
        disabled.pop_last();
    }
    allowed.retain(|id, _| valid_id(id));
    for ps in allowed.values_mut() {
        ps.retain(|p| Permission::from_id(p).is_some());
    }
    allowed.retain(|_, ps| !ps.is_empty());
    while allowed.len() > MAX_CHOICES {
        allowed.pop_last();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("solidcraft-plugins-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, folder: &str, body: &str) {
        let d = dir.join(folder);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(MANIFEST), body).unwrap();
    }

    #[test]
    fn a_folder_with_plugin_json_is_listed_and_a_bad_one_is_skipped() {
        let dir = temp("scan");
        write(
            &dir,
            "ftc-frc-parts",
            r#"{"id":"ftc-frc-parts","name":"FTC and FRC parts","version":"0.1.0","description":"Commercial robot parts."}"#,
        );
        write(&dir, "broken", r#"{"id":"other","name":"Nope","version":"1"}"#);
        write(&dir, "newer", r#"{"id":"newer","name":"Newer","version":"1","entry":"main.wasm"}"#);
        std::fs::create_dir_all(dir.join("notes")).unwrap();
        std::fs::create_dir_all(dir.join(".cache")).unwrap();
        let (plugins, skipped) = scan(&dir);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].id, "ftc-frc-parts");
        assert_eq!(plugins[0].name, "FTC and FRC parts");
        assert!(skipped.iter().any(|s| s.folder == "broken" && s.reason.contains("does not match")));
        assert!(skipped.iter().any(|s| s.folder == "notes" && s.reason.contains("no plugin.json")));
        let newer = skipped.iter().find(|s| s.folder == "newer").unwrap();
        assert!(newer.reason.contains("newer SolidCraft"), "{}", newer.reason);
        assert!(!skipped.iter().any(|s| s.folder == ".cache"), "hidden folders are not plugins");
        assert!(scan(&dir.join("missing")).0.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ids_and_manifests_refuse_what_would_not_fit_on_the_page() {
        assert!(valid_id("a"));
        assert!(valid_id("ftc-frc-parts"));
        assert!(!valid_id(""));
        assert!(!valid_id("-a"));
        assert!(!valid_id("a-"));
        assert!(!valid_id("FTC"));
        assert!(!valid_id("a/b"));
        assert!(!valid_id(&"a".repeat(65)));
        let manifest = |name: &str, description: &str| Manifest {
            id: "gear".into(),
            name: name.into(),
            version: "1".into(),
            description: description.into(),
            permissions: vec![],
        };
        assert!(check(manifest("Gear\nbox", ""), "gear").is_err());
        assert!(check(manifest("Gear", &"x".repeat(241)), "gear").unwrap_err().contains("240"));
        assert!(check(manifest("Gear", ""), "gear").is_ok());
    }

    #[test]
    fn a_permission_this_version_does_not_know_skips_the_plugin() {
        let dir = temp("permissions");
        write(&dir, "turnpad", r#"{"id":"turnpad","name":"Turnpad","version":"1.1.0","permissions":["control","control"]}"#);
        write(&dir, "greedy", r#"{"id":"greedy","name":"Greedy","version":"1","permissions":["control","files"]}"#);
        let (plugins, skipped) = scan(&dir);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].permissions, vec![Permission::Control], "a repeated permission counts once");
        let greedy = skipped.iter().find(|s| s.folder == "greedy").unwrap();
        assert!(greedy.reason.contains("`files`"), "{}", greedy.reason);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_permission_is_used_only_once_the_user_allows_it_and_while_the_plugin_is_on() {
        let plugin = |id: &str, permissions: Vec<Permission>| Plugin {
            id: id.into(),
            name: id.into(),
            version: "1".into(),
            description: String::new(),
            permissions,
        };
        let plugins = [plugin("turnpad", vec![Permission::Control]), plugin("parts", vec![])];
        let mut disabled = BTreeSet::new();
        let mut allowed = BTreeMap::new();
        let choices = Choices { disabled: &disabled, allowed: &allowed };
        assert_eq!(choices.state(&plugins[0]), State::NeedsApproval, "installing a plugin grants nothing");
        assert_eq!(choices.missing(&plugins[0]), vec![Permission::Control]);
        assert_eq!(choices.state(&plugins[1]), State::On, "a plugin that asks for nothing is on");
        assert!(choices.using(&plugins, Permission::Control).is_empty());
        allowed.insert("turnpad".to_string(), BTreeSet::from(["control".to_string()]));
        let choices = Choices { disabled: &disabled, allowed: &allowed };
        assert_eq!(choices.using(&plugins, Permission::Control), vec!["turnpad".to_string()]);
        disabled.insert("turnpad".to_string());
        let choices = Choices { disabled: &disabled, allowed: &allowed };
        assert_eq!(choices.state(&plugins[0]), State::Off);
        assert!(choices.using(&plugins, Permission::Control).is_empty(), "turned off: nothing is used");
    }

    #[test]
    fn choices_from_a_hand_edited_file_are_bounded() {
        let mut disabled: BTreeSet<String> = (0..300).map(|i| format!("p{i}")).collect();
        disabled.insert("Bad Id".into());
        let mut allowed = BTreeMap::from([
            ("turnpad".to_string(), BTreeSet::from(["control".to_string(), "files".to_string()])),
            ("odd".to_string(), BTreeSet::from(["files".to_string()])),
        ]);
        assert!(check_choices(Choices { disabled: &disabled, allowed: &allowed }).is_err());
        limit_choices(&mut disabled, &mut allowed);
        assert_eq!(disabled.len(), MAX_CHOICES);
        assert!(!disabled.contains("Bad Id"));
        assert_eq!(allowed, BTreeMap::from([("turnpad".to_string(), BTreeSet::from(["control".to_string()]))]));
        assert!(check_choices(Choices { disabled: &disabled, allowed: &allowed }).is_ok());
    }
}
