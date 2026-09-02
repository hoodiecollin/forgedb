mod project;

use project::ProjectDto;
use serde::Serialize;

#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
fn load_project(schema_path: String, data_dir: Option<String>) -> Result<ProjectDto, String> {
    project::load_project(&schema_path, data_dir.as_deref())
}

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
