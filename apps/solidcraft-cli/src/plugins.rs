//! `solidcraft-cli plugins`: the plugins installed for this user (docs/plugins.md), and a check of
//! a plugin folder for its author. Whether each one is on and what the user allowed is kept in the
//! desktop app's preferences (Preferences › Plugins), which this command does not read.

use std::path::Path;

use serde_json::json;
use solidcraft_engine::{control_auth, plugins};

const USAGE: &str = "usage: solidcraft-cli plugins [list [--json] | check <folder> | dir | endpoint <id>]";

pub fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        None | Some("list") => {
            let dir = plugins::plugins_dir().ok_or("no plugins folder on this system")?;
            let (found, skipped) = plugins::scan(&dir);
            if args.iter().any(|a| a == "--json") {
                println!("{}", super::pretty(&json!({ "dir": dir.to_string_lossy(), "plugins": found, "skipped": skipped })));
                return Ok(());
            }
            println!("plugins in {}", dir.display());
            if found.is_empty() {
                println!("  (none)");
            }
            for p in &found {
                let asks = p.permissions.iter().map(|p| p.id()).collect::<Vec<_>>().join(", ");
                let asks = if asks.is_empty() { String::new() } else { format!("  asks for: {asks}") };
                println!("  {}  {} {}{asks}", p.id, p.name, p.version);
            }
            for s in &skipped {
                println!("  skipped {}: {}", s.folder, s.reason);
            }
            Ok(())
        }
        Some("check") => {
            let folder = args.get(1).ok_or_else(|| USAGE.to_string())?;
            let p = plugins::read(Path::new(folder))?;
            println!("{}", super::pretty(&json!(p)));
            Ok(())
        }
        Some("dir") => {
            println!("{}", plugins::plugins_dir().ok_or("no plugins folder on this system")?.display());
            Ok(())
        }
        Some("endpoint") => {
            let id = args.get(1).ok_or_else(|| USAGE.to_string())?;
            let path = control_auth::plugin_endpoint_path(id).ok_or_else(|| format!("`{id}` is not a plugin id"))?;
            let endpoint = control_auth::read_plugin_endpoint(id)
                .ok_or_else(|| format!("no endpoint in {}: the app is not running, or `{id}` is off or not allowed `control`", path.display()))?;
            // The token stays in the file; print where it is, not the token itself.
            println!("{}", super::pretty(&json!({ "port": endpoint.port, "pid": endpoint.pid, "file": path.to_string_lossy() })));
            Ok(())
        }
        _ => Err(USAGE.to_string()),
    }
}
