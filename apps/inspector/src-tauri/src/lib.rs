//! ForgeDB Inspector — Tauri desktop shell.
//!
//! The frontend is a static Next.js export (`apps/inspector/out`) rendered in the
//! webview. This backend exposes `#[tauri::command]`s over Tauri IPC. Today it is
//! the shell only plus the at-rest **Structure lens** (#12): commands that read a
//! project's `.forge` schema via the workspace `forgedb-parser` crate and its
//! on-disk storage stats via `forgedb-compaction` — all tooling reads, never a
//! runtime schema engine. The **Live lens** (#13) talks to a running generated API
//! over HTTP/WebSocket directly from the frontend.

mod project;

use project::ProjectDto;
use serde::Serialize;

/// Returns the inspector's package version — smoke-tests the IPC bridge and gives
/// the frontend a single source of truth for the "About" surface.
#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Load a project's Structure lens: parse its `.forge` schema and, when a data
/// directory is supplied, read per-model storage stats. See [`project`].
#[tauri::command]
fn load_project(schema_path: String, data_dir: Option<String>) -> Result<ProjectDto, String> {
    project::load_project(&schema_path, data_dir.as_deref())
}

/// A project to auto-open on launch, from `FORGEDB_INSPECTOR_PROJECT` (schema
/// path) and optional `FORGEDB_INSPECTOR_DATA` (data dir). Powers `open-with` /
/// `forgedb-inspector <schema>` and a dev default; `None` ⇒ the shell starts on
/// its built-in sample.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartupProject {
    schema_path: String,
    data_dir: Option<String>,
}

#[tauri::command]
fn startup_project() -> Option<StartupProject> {
    let schema_path = std::env::var("FORGEDB_INSPECTOR_PROJECT").ok()?;
    let data_dir = std::env::var("FORGEDB_INSPECTOR_DATA").ok().filter(|s| !s.is_empty());
    Some(StartupProject { schema_path, data_dir })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_websocket::init())
        .invoke_handler(tauri::generate_handler![
            app_version,
            load_project,
            startup_project
        ])
        .run(tauri::generate_context!())
        .expect("error while running the ForgeDB Inspector");
}
