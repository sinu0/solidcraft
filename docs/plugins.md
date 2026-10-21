# Plugins

A plugin is a folder the user installs; SolidCraft lists it in **Preferences › Plugins**, where
it can be turned off, and gives it only what the user allows. What works today is the
**manifest** and its **permissions**; running plugin code in a sandbox (WebAssembly, parametric
timeline features) is designed below and not built yet
([#1221](https://github.com/bherbruck/solidcraft/issues/1221)). Nothing in a plugin folder is run.

The code: `solidcraft_engine::plugins` (manifests, scan, which plugins are in use),
`solidcraft_engine::control_auth` (endpoint files), `crates/ui-egui/src/plugins.rs` (the page),
`solidcraft-cli plugins`.

## Installing

Copy the plugin's folder into the plugins folder:

| System | Plugins folder |
|---|---|
| Linux | `$XDG_DATA_HOME/solidcraft/plugins` (`~/.local/share/solidcraft/plugins`) |
| macOS | `~/Library/Application Support/SolidCraft/plugins` |
| Windows | `%LOCALAPPDATA%\SolidCraft\plugins` |

`SOLIDCRAFT_PLUGINS_DIR` replaces it (tests, portable installs). **Show in folder** on the page
opens it; `solidcraft-cli plugins dir` prints it. The page notices a new folder within a couple
of seconds. A folder whose name starts with `.` is ignored; a folder that is not a plugin is
listed under the plugins with the reason.

## Manifest

`<plugins folder>/<id>/plugin.json`, at most 16 KiB:

```json
{"id": "turnpad", "name": "Turnpad", "version": "1.1.0", "description": "Dial and buttons for the view.", "permissions": ["control"]}
```

| Member | Required | Rule |
|---|---|---|
| `id` | yes | the folder's name: 1–64 lowercase letters, digits and hyphens, not starting or ending with a hyphen |
| `name` | yes | 1–80 characters, one line |
| `version` | yes | 1–32 letters, digits, `.`, `-`, `+` |
| `description` | no | at most 240 characters, one line |
| `permissions` | no | ids from the table below; a repeat counts once |

Any other member, or a permission this version does not know, makes the folder a skipped one
(the reason says it may be for a newer SolidCraft), so a plugin never runs with less than it
asked for. Check a folder before shipping it:

```text
$ solidcraft-cli plugins check ./turnpad
{"id": "turnpad", "name": "Turnpad", "version": "1.1.0", "description": "…", "permissions": ["control"]}
```

`solidcraft-cli plugins list [--json]` shows what the app will find, skipped folders included.

## Permissions

A manifest asks; the user allows. Installing a plugin grants nothing: a plugin that asks for a
permission is listed as *off until you allow what it asks for*, with **Allow**, and is used only
after the user clicks it. **Take back** withdraws it. The choices are kept in the preferences:
`disabled_plugins` (ids turned off) and `plugin_permissions` (`{"<id>": ["control"]}`), which a
control-channel client can read and set with `ui.prefs`. Restore Defaults on the page turns every
plugin back on and takes back every permission. Allowed permissions are checked against what the
manifest asks for at every start, so an update that asks for more is held until the user allows
that too. `--safe-mode` uses no plugins.

| Permission | What it does |
|---|---|
| `control` | The [control channel](control-protocol.md) is opened at start, and its port and token are left for the plugin's program (below). |

Changes take effect at the next start.

### `control`: a program outside SolidCraft

For a plugin that stands for a program of its own (a dial or 3D-mouse driver, a test rig, a
script runner), which otherwise needs `SOLIDCRAFT_CONTROL_PORT` in the user's session. While such
a plugin is on and allowed `control`, the desktop app opens the channel at start: on the port of
`--control`/`SOLIDCRAFT_CONTROL_PORT` when one is given, else on a free loopback port. It then
writes the **endpoint file** `<control folder>/plugin-<id>.json`, readable only by the user (the
folder of the token files, [control-protocol.md](control-protocol.md#token)):

```json
{"port": 41237, "token": "…64 hex…", "pid": 12345}
```

The program reads it, connects to `127.0.0.1:<port>` and sends `token` on its first request. The
file is removed when the app ends (unless a later launch rewrote it); `pid` lets the program tell
a file left by a crash from a running app. With several windows the latest launch wins. A
missing file means the app is not running, or the plugin is off or not allowed. A minimal client:

```python
import json, socket
from pathlib import Path
ep = json.loads((Path.home() / ".local/share/solidcraft/control/plugin-turnpad.json").read_text())
s = socket.create_connection(("127.0.0.1", ep["port"]))
s.sendall((json.dumps({"id": 1, "method": "ui.inspect", "params": {}, "token": ep["token"]}) + "\n").encode())
print(s.makefile().readline())
```

`solidcraft-cli plugins endpoint <id>` prints the port and the file (not the token). The program
itself is installed and started by its own installer; SolidCraft does not start it.

## Adding a permission (for SolidCraft developers)

1. Add a variant to `solidcraft_engine::plugins::Permission` (`ALL`, `id`) and act on it where
   the app starts, with `Choices::using(plugins, Permission::…)`.
2. Explain it on the page (`explain` in `crates/ui-egui/src/plugins.rs`, a `ui.plugins_<id>`
   message) and in the table above.
3. Test that it is used only while the plugin is on and allowed, and never in safe mode.

## Design: sandboxed code and parametric features (not built yet)

Design companion to [#1221](https://github.com/bherbruck/solidcraft/issues/1221): sandboxed
WebAssembly plugins with a WIT interface, the same `run(command, args)` JSON as
[MCP](mcp.md) and the [control protocol](control-protocol.md). Browser hosting is in
[web-runtime.md](web-runtime.md). It adds **parametric timeline features**: a generic host-owned
feature shell that plugins fill with parameters, display metadata, and rebuild logic, without a
second timeline model or a private geometry API. The wasm host will extend the manifest above
(an entry module, its own permissions) and honour the same on/off and allow choices.

### Goals (unchanged from #1221)

- Sandboxed by default: manifest permissions, fuel/memory limits on desktop, broker enforcement on web.
- Any language that compiles to Wasm components; Rust first.
- **One command surface:** plugins change the model only through engine commands (undoable, journaled).
- Versioned WIT world (`solidcraft:plugin@1.0.0`).

### Two plugin shapes

| Shape | Timeline | Rebuild | Milestone |
|-------|----------|---------|-----------|
| **One-shot** | Optional; often no persistent feature | N/A | 1 (first ship) |
| **Parametric** | One **`Plugin` feature row** per plugin instance | Host calls plugin on param/timeline rebuild | 2 |

One-shot plugins call `execute(params)` once; they may emit ordinary native features (sketch,
extrude, …) inside that transaction. Parametric plugins still use `run()` inside rebuild; the
difference is that the document **records** a single feature whose params the user edits later.

### Generic feature shell (host-owned)

Add one variant to the parametric document (name illustrative):

```text
FeatureKind::Plugin {
  plugin_id: String,           // "gear_generator"
  api_version: u32,            // matches WIT world
  params: Value,               // JSON owned by the plugin schema
  display: PluginDisplay,      // timeline + browser presentation
  provenance: PluginProvenance, // optional: manifest hash, build id
}
```

**`PluginDisplay`** (stored in the file, not recomputed from plugin code at open):

| Field | Role |
|-------|------|
| `label` | Primary timeline / browser text |
| `icon_id` | Host icon registry name (same system as [`feature_icon`](https://github.com/bherbruck/solidcraft/blob/main/crates/ui-egui/src/workspace.rs)) |
| `tooltip` | Hover text, `inspect` type string |
| `type_name` | short feature type name for power users |

The timeline renders **any** `Plugin` feature through one code path: read `display`, draw icon +
label, wire suppress/rename/delete/reorder like built-in features. No per-plugin egui in milestone 2.

Optional later: **`parameter_schema()`** in WIT returns `{ id, label, kind, min, max, … }` so the
host renders sliders and dimension rows without custom UI per plugin.

### WIT entry points (phase 2 additions)

Milestone 1 (#1221): `manifest()`, `execute(params) -> result<string, string>`.

Milestone 2 parametric:

| Export | Purpose |
|--------|---------|
| `evaluate(params, ctx) -> result<string, string>` | Rebuild: plugin runs `run()` commands; result is success JSON or error |
| `parameter_schema() -> string` | Optional JSON schema for generic parameter UI |
| `fallback_policy() -> string` | Declares missing-plugin behaviour (see below) |

`ctx` is host-provided read-only hints (configuration name, units, feature id); plugins must not
 rely on it for correctness.

Inside **`evaluate`**, the plugin uses the same **`run(command, args)`** import as one-shot mode.
The host applies commands to the current eval context (or a scratch session merged by the host).
No second registry of geometry ops.

### Rebuild flow

```text
Timeline rebuild reaches FeatureKind::Plugin
  -> load plugin (wasmtime desktop / broker web) if permitted + present
  -> evaluate(stored params)
       -> plugin calls run("sketch.line", …), run("solid.extrude", …), …
  -> host merges geometry like any native feature
  -> errors surface on the feature row like native eval errors
```

Param changes (parameter panel, `parameters.change`, config switch) re-run **`evaluate`** for
affected plugin features only when dependencies say so (same invalidation rules as native features).

### Missing plugin and API drift

Manifest declares **`fallback_policy`**:

| Policy | Behaviour |
|--------|-----------|
| `recorded_mesh` | Optional B-rep or mesh blob stored beside params; open works, edit disabled without plugin |
| `recorded_script` | Stored list of native commands; rebuild without plugin if every command is built-in |
| `suppress_with_warning` | Feature suppressed; user sees clear message |
| `error_on_rebuild` | Hard fail on eval (strict teams) |

Design files shared in CollabCraft should prefer a policy that **opens safely** (`recorded_mesh` or
`recorded_script`). CI for plugins (#1221) should include: uninstall plugin, reopen file, assert
document still opens and policy matches manifest.

### Icons and metadata

Plugins declare allowed **`icon_id`** values in `manifest()`; the host rejects unknown ids. Icons
stay in the app registry so untrusted plugins cannot inject arbitrary raster assets into the tree
without a reviewed icon pack.

**Rename** by the user updates the feature user label; **`display.label`** remains the default from
creation unless the plugin updates it via a host callback (optional future).

Command naming: prefer **`plugin_id.function`** in `run()` or registered command aliases to avoid
collisions ([#1221 discussion](https://github.com/bherbruck/solidcraft/issues/1221)).

### Group header variant (lighter-weight v2a)

Before full **`evaluate`**, a stepping stone: one-shot plugin creates a **feature folder** whose
header carries `PluginDisplay` metadata while children are ordinary native features. Rebuild does
not need the plugin until the user edits plugin-owned params on the header. Migrates cleanly to
full `FeatureKind::Plugin` when `evaluate` lands.

### Relationship to craft-libs

Plugin **document model and eval** stay in SolidCraft (`crates/doc`, `crates/engine`). craft-libs
provides worker/broker hosting only ([web-runtime.md](web-runtime.md)). Neutral serde types
(`VersionedParams`, `PluginDisplay`) may move to a shared crate later if multiple apps need them;
CAD plugins remain SolidCraft-specific.

### Phasing

1. WIT + wasmtime host + one-shot `execute` + permissions tests (#1221 milestone 1).
2. `FeatureKind::Plugin` + `PluginDisplay` + timeline generic row + `evaluate` + fallback policy.
3. Web broker runs same WIT; parametric features offline only if plugin module cached and policy
   allows.
4. Parameter schema UI, cloud-signed plugin index, inspector extensions.

### Tests (parametric)

- Create plugin feature, change param, rebuild, measure geometry (oracle or bbox).
- Undo/redo across plugin eval.
- Remove plugin binary, reopen design, assert fallback policy.
- Icon id from manifest appears on timeline (scenario or screenshot harness).
