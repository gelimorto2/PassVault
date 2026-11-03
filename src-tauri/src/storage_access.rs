use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

#[cfg(target_os = "android")]
use serde::Deserialize;
#[cfg(target_os = "android")]
use tauri::{plugin::PluginHandle, Manager};

#[cfg(target_os = "android")]
pub struct StorageAccess<R: Runtime>(PluginHandle<R>);

#[cfg(target_os = "android")]
impl<R: Runtime> StorageAccess<R> {
    pub fn has_access(&self) -> bool {
        #[derive(Deserialize)]
        struct Resp {
            granted: bool,
        }
        self.0
            .run_mobile_plugin::<Resp>("hasAccess", ())
            .map(|r| r.granted)
            .unwrap_or(false)
    }

    pub fn request_access(&self) {
        let _ = self.0.run_mobile_plugin::<serde_json::Value>("requestAccess", ());
    }
}

/// Bridges to the local `StorageAccessPlugin.kt` (see gen/android), which can
/// request Android's "All files access" permission so the app can look for
/// existing vault files outside its own sandbox. No-op on desktop, where
/// the filesystem is already freely accessible.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("storage-access")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                let handle = _api
                    .register_android_plugin("com.leo.passvault.storage", "StorageAccessPlugin")?;
                _app.manage(StorageAccess(handle));
            }
            Ok(())
        })
        .build()
}

#[tauri::command]
pub fn has_full_file_access(app: tauri::AppHandle) -> bool {
    #[cfg(target_os = "android")]
    {
        app.try_state::<StorageAccess<tauri::Wry>>()
            .map(|s| s.has_access())
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        true
    }
}

#[tauri::command]
pub fn request_full_file_access(app: tauri::AppHandle) {
    #[cfg(target_os = "android")]
    {
        if let Some(s) = app.try_state::<StorageAccess<tauri::Wry>>() {
            s.request_access();
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
    }
}

/// Shallow scan of a few well-known Android directories (Downloads,
/// Documents, common sync-app folders) for files that look like PassVault
/// vaults. Only useful once `has_full_file_access` is true; on desktop, or
/// without the permission, these paths simply won't exist/be readable and
/// this returns an empty list.
#[tauri::command]
pub fn scan_for_vault_files() -> Vec<String> {
    let candidate_dirs = [
        "/storage/emulated/0/Download",
        "/storage/emulated/0/Documents",
        "/storage/emulated/0/",
        "/storage/emulated/0/Syncthing",
        "/storage/emulated/0/Sync",
    ];
    let mut found = Vec::new();
    for dir in candidate_dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.to_lowercase().contains("vault") {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                continue;
            };
            if value.get("version").is_some() && value.get("blob").is_some() {
                found.push(path.to_string_lossy().to_string());
            }
        }
    }
    found
}
