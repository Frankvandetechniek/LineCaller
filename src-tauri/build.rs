// Declaring the app commands here makes Tauri generate one permission per
// command (allow-<name>). Only the ones listed in capabilities/default.json
// are callable from the UI.
const COMMANDS: &[&str] = &[
    "get_bootstrap",
    "save_settings",
    "connect",
    "disconnect",
    "dial",
    "accept_call",
    "set_audio",
    "drop_line",
    "set_audio_lock",
    "set_incoming_block",
    "refresh_lines",
    "clear_recents",
    "simulate_incoming_call",
    "export_config",
    "import_config",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
