// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // WebKitGTK on native Wayland drops ordinary key presses here (only
    // dead keys like AZERTY's ^ got through), so run under XWayland unless
    // the user explicitly chose a backend. Must happen before GTK starts.
    #[cfg(target_os = "linux")]
    if std::env::var_os("GDK_BACKEND").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_some() {
        std::env::set_var("GDK_BACKEND", "x11");
    }
    tauri_app_lib::run()
}
