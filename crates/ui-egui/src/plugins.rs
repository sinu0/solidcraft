//! Preferences › Plugins: packages installed under the per-user data folder.
//!
//! What a plugin is, how its manifest is checked and which plugins are in use is
//! [`solidcraft_engine::plugins`]; this page lists them, turns them on and off
//! (`disabled_plugins`) and lets the user allow what a plugin asks for (`plugin_permissions`).
//! It does not load or run anything from a plugin folder.

use std::path::Path;

use egui::RichText;
use solidcraft_engine::plugins::{self, Choices, Permission, Plugin, Skipped, State};

use crate::SolveApp;

/// How often the page looks at the plugins folder again while it is shown, in seconds.
const RESCAN_S: f64 = 2.0;

/// The user's choices as the engine reads them.
pub fn choices(app: &SolveApp) -> Choices<'_> {
    Choices { disabled: &app.preferences.disabled_plugins, allowed: &app.preferences.plugin_permissions }
}

/// The installed plugins that are on and allowed the control channel, by id. Desktop only; empty
/// when there is no plugins folder.
pub fn control_plugins(app: &SolveApp) -> Vec<String> {
    let Some(dir) = plugins::plugins_dir() else {
        return Vec::new();
    };
    control_plugins_in(&dir, choices(app))
}

fn control_plugins_in(dir: &Path, choices: Choices<'_>) -> Vec<String> {
    choices.using(&plugins::scan(dir).0, Permission::Control)
}

/// What one permission lets a plugin do, for the page.
fn explain(permission: Permission) -> &'static str {
    match permission {
        Permission::Control => crate::i18n::tr("ui.plugins_control"),
    }
}

/// The folder's plugins, read again at most every [`RESCAN_S`] while the page is drawn, so the
/// page does not read the disk every frame yet shows a plugin soon after it is copied in.
fn scanned(ui: &egui::Ui, dir: &Path) -> (Vec<Plugin>, Vec<Skipped>) {
    let id = egui::Id::new("prefs:plugins:scan");
    let now = ui.input(|i| i.time);
    if let Some((at, found)) = ui.data(|d| d.get_temp::<(f64, (Vec<Plugin>, Vec<Skipped>))>(id))
        && (0.0..RESCAN_S).contains(&(now - at))
    {
        return found;
    }
    let found = plugins::scan(dir);
    ui.data_mut(|d| d.insert_temp(id, (now, found.clone())));
    ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(RESCAN_S));
    found
}

/// Allow (or take back) everything `plugin` asks for.
fn allow(app: &mut SolveApp, plugin: &Plugin, on: bool) {
    let prefs = &mut app.preferences;
    if !on {
        prefs.plugin_permissions.remove(&plugin.id);
    } else if prefs.plugin_permissions.len() < plugins::MAX_CHOICES || prefs.plugin_permissions.contains_key(&plugin.id) {
        prefs.plugin_permissions.insert(plugin.id.clone(), plugin.permissions.iter().map(|p| p.id().to_string()).collect());
    }
}

/// The Plugins page. Desktop only: the browser build has no per-user plugin folder.
pub fn page(app: &mut SolveApp, ui: &mut egui::Ui) {
    let t = crate::theme::Tokens::get();
    let Some(dir) = plugins::plugins_dir() else {
        ui.label(crate::i18n::tr("ui.plugins_desktop"));
        return;
    };
    let path = dir.display().to_string();
    ui.add(egui::Label::new(RichText::new(crate::i18n::format("ui.plugins_note", &[("path", &path)])).size(t.font_small).color(t.text_dim)).wrap());
    ui.add_space(8.0);
    if crate::ui_kit::button(ui, crate::i18n::tr_label("Show in folder"), crate::ui_kit::Button::Normal).clicked() {
        // `show_in_folder` opens the parent of the path it is given, so the argument is a name
        // inside the plugins directory (it need not exist).
        if let Err(e) = std::fs::create_dir_all(&dir) {
            app.set_status(format!("Could not create the plugins folder: {e}"), true);
        } else if let Some(show) = &app.services.show_in_folder {
            show(&dir.join(plugins::MANIFEST).display().to_string());
        }
    }
    ui.add_space(8.0);
    let (plugins, skipped) = scanned(ui, &dir);
    if plugins.is_empty() {
        ui.label(crate::i18n::tr("ui.plugins_empty"));
    }
    for plugin in &plugins {
        let state = choices(app).state(plugin);
        let mut on = state != State::Off;
        ui.horizontal(|ui| {
            let check = crate::ui_kit::checkbox(ui, "", &mut on);
            crate::scenario::publish_handle(&format!("prefs:plugin:{}", plugin.id), check.rect.center());
            if check.changed() {
                if on {
                    app.preferences.disabled_plugins.remove(&plugin.id);
                } else if app.preferences.disabled_plugins.len() < plugins::MAX_CHOICES {
                    app.preferences.disabled_plugins.insert(plugin.id.clone());
                }
            }
            ui.vertical(|ui| {
                ui.label(RichText::new(&plugin.name).strong());
                ui.label(
                    RichText::new(crate::i18n::format("ui.plugins_version", &[("version", &plugin.version)])).size(t.font_small).color(t.text_dim),
                );
                if !plugin.description.is_empty() {
                    ui.label(RichText::new(&plugin.description).size(t.font_small).color(t.text_dim));
                }
                for permission in &plugin.permissions {
                    ui.add(egui::Label::new(RichText::new(explain(*permission)).size(t.font_small).color(t.text_dim)).wrap());
                }
                if plugin.permissions.is_empty() {
                    return;
                }
                let allowed = choices(app).missing(plugin).is_empty();
                if !allowed {
                    ui.label(RichText::new(crate::i18n::tr("ui.plugins_needs_approval")).size(t.font_small).color(t.warning));
                }
                let (label, handle) = if allowed { ("ui.plugins_revoke", "revoke") } else { ("ui.plugins_allow", "allow") };
                let button = crate::ui_kit::button(ui, crate::i18n::tr(label), crate::ui_kit::Button::Normal);
                crate::scenario::publish_handle(&format!("prefs:plugin:{}:{handle}", plugin.id), button.rect.center());
                if button.clicked() {
                    allow(app, plugin, !allowed);
                }
            });
        });
        ui.separator();
    }
    if !skipped.is_empty() {
        ui.add_space(6.0);
        ui.label(
            RichText::new(crate::i18n::format("ui.plugins_skipped", &[("count", &skipped.len().to_string())])).size(t.font_small).color(t.text_dim),
        );
        for skip in skipped.iter().take(8) {
            let who = if skip.folder.is_empty() { "folder" } else { skip.folder.as_str() };
            ui.label(RichText::new(format!("{who}: {}", skip.reason)).size(t.font_small).color(t.text_dim));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, folder: &str, body: &str) {
        let d = dir.join(folder);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(plugins::MANIFEST), body).unwrap();
    }

    #[test]
    fn only_a_plugin_that_is_on_and_allowed_control_opens_the_control_channel() {
        let dir = std::env::temp_dir().join(format!("solidcraft-plugins-control-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        write(&dir, "turnpad", r#"{"id":"turnpad","name":"Turnpad","version":"1.1.0","permissions":["control"]}"#);
        write(&dir, "parts", r#"{"id":"parts","name":"Parts","version":"1"}"#);
        let mut app = crate::SolveApp::new(solidcraft_engine::Session::default(), crate::Services::default());
        assert!(control_plugins_in(&dir, choices(&app)).is_empty(), "installed but not allowed: no channel");
        let (found, _) = plugins::scan(&dir);
        let turnpad = found.iter().find(|p| p.id == "turnpad").unwrap();
        allow(&mut app, turnpad, true);
        assert_eq!(control_plugins_in(&dir, choices(&app)), vec!["turnpad".to_string()]);
        app.preferences.disabled_plugins.insert("turnpad".into());
        assert!(control_plugins_in(&dir, choices(&app)).is_empty(), "turned off: no channel");
        app.preferences.disabled_plugins.clear();
        allow(&mut app, turnpad, false);
        assert!(control_plugins_in(&dir, choices(&app)).is_empty(), "taken back: no channel");
        assert!(control_plugins_in(&dir.join("missing"), choices(&app)).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plugin_choices_are_saved_and_restore_defaults_clears_them() {
        use serde_json::json;
        let mut app = crate::SolveApp::new(solidcraft_engine::Session::default(), crate::Services::default());
        let on = |app: &SolveApp, id: &str| !app.preferences.disabled_plugins.contains(id);
        assert!(on(&app, "ftc-frc-parts"));
        crate::prefs::set(&mut app, &json!({"disabled_plugins": ["ftc-frc-parts"], "plugin_permissions": {"turnpad": ["control"]}})).unwrap();
        assert!(!on(&app, "ftc-frc-parts"));
        let err = crate::prefs::set(&mut app, &json!({"disabled_plugins": ["Bad Id"]})).unwrap_err();
        assert!(err.contains("disabled_plugins"), "{err}");
        let err = crate::prefs::set(&mut app, &json!({"plugin_permissions": {"turnpad": ["files"]}})).unwrap_err();
        assert!(err.contains("plugin_permissions"), "{err}");
        assert!(!on(&app, "ftc-frc-parts"), "a refused list leaves the previous choice");
        let mut other = crate::SolveApp::new(solidcraft_engine::Session::default(), crate::Services::default());
        other.load_prefs(&app.prefs());
        assert!(!on(&other, "ftc-frc-parts"));
        assert!(other.preferences.plugin_permissions.get("turnpad").is_some_and(|p| p.contains("control")));
        crate::prefs::control(&mut app, &json!({"open": "Plugins", "button": "defaults"})).unwrap();
        assert_eq!(app.prefs_window.section, crate::prefs::PAGE_PLUGINS);
        assert!(on(&app, "ftc-frc-parts"));
        assert!(app.preferences.plugin_permissions.is_empty(), "restore defaults takes back every permission");
        assert!(crate::prefs::control(&mut app, &json!({"open": "NoSuch"})).is_err());
    }
}
