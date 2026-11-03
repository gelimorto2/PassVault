//! Bridges to the local `SystemPlugin.kt` (Android): autofill-service status
//! and the "choose autofill service" settings screen (Samsung: Settings →
//! General management → Passwords and autofill). No-op elsewhere.

use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

#[cfg(target_os = "android")]
use tauri::{plugin::PluginHandle, Manager};

#[cfg(target_os = "android")]
pub struct System<R: Runtime>(PluginHandle<R>);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("system")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                let handle = _api.register_android_plugin("com.leo.passvault.system", "SystemPlugin")?;
                _app.manage(System(handle));
            }
            Ok(())
        })
        .build()
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct AutofillStatus {
    pub supported: bool,
    pub enabled: bool,
}

#[tauri::command]
pub async fn autofill_status(app: tauri::AppHandle) -> AutofillStatus {
    #[cfg(target_os = "android")]
    {
        if let Some(s) = app.try_state::<System<tauri::Wry>>() {
            return s
                .0
                .run_mobile_plugin::<AutofillStatus>("autofillStatus", ())
                .unwrap_or_default();
        }
    }
    let _ = app;
    AutofillStatus::default()
}

#[tauri::command]
pub async fn open_autofill_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::AppStateM>,
) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        crate::hold_unlock(&state, 120);
        if let Some(s) = app.try_state::<System<tauri::Wry>>() {
            return s
                .0
                .run_mobile_plugin::<serde_json::Value>("openAutofillSettings", ())
                .map(|_| ())
                .map_err(|e| e.to_string());
        }
    }
    let _ = (app, state);
    Err("Autofill settings only exist on Android.".into())
}
