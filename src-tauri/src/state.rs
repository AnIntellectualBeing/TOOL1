//! Shared application state and file locations.

use crate::android::AndroidSession;
use crate::iphone::Receiver;
use crate::mdns::MdnsDiscovery;
use crate::proc::{AdbRunner, LogTail};
use phonemirror_core::settings::Settings;
use phonemirror_core::store::{load_json, save_json, DeviceStore};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize)]
pub struct Paths {
    pub data: PathBuf,
    pub adb: PathBuf,
    pub scrcpy: PathBuf,
    pub uxplay: PathBuf,
    /// Folder of the bundled GStreamer runtime (Windows), if present.
    pub gst_root: Option<PathBuf>,
    pub app_exe: PathBuf,
    pub default_screenshots: PathBuf,
    pub default_recordings: PathBuf,
}

const EXE: &str = std::env::consts::EXE_SUFFIX;

/// Bundled engine if present, else the same program on PATH (handy when
/// developing on Linux/macOS with scrcpy/uxplay installed).
fn engine(engines: &Path, sub: &str, name: &str) -> PathBuf {
    let bundled = engines.join(sub).join(format!("{name}{EXE}"));
    if bundled.exists() {
        return bundled;
    }
    if let Some(found) = std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join(format!("{name}{EXE}")))
            .find(|c| c.is_file())
    }) {
        return found;
    }
    bundled
}

impl Paths {
    pub fn resolve(app: &AppHandle) -> Self {
        let resources = app
            .path()
            .resource_dir()
            .unwrap_or_else(|_| PathBuf::from("."));
        let engines = std::env::var_os("PHONEMIRROR_ENGINES")
            .map(PathBuf::from)
            .unwrap_or_else(|| resources.join("engines"));
        let data = app
            .path()
            .app_data_dir()
            .unwrap_or_else(|_| PathBuf::from("phonemirror-data"));
        let pictures = app.path().picture_dir().unwrap_or_else(|_| data.clone());
        let videos = app.path().video_dir().unwrap_or_else(|_| data.clone());
        let uxplay = engine(&engines, "iphone", "uxplay");
        let gst_root = uxplay
            .parent()
            .map(|p| p.to_path_buf())
            .filter(|p| p.join("lib").join("gstreamer-1.0").is_dir());
        Self {
            adb: engine(&engines, "android", "adb"),
            scrcpy: engine(&engines, "android", "scrcpy"),
            uxplay,
            gst_root,
            app_exe: std::env::current_exe().unwrap_or_default(),
            default_screenshots: pictures.join("PhoneMirror"),
            default_recordings: videos.join("PhoneMirror"),
            data,
        }
    }
}

pub struct Shared {
    pub app: AppHandle,
    pub paths: Paths,
    pub settings: Mutex<Settings>,
    pub devices: Mutex<DeviceStore>,
    pub discovery: MdnsDiscovery,
    pub iphone: Mutex<Receiver>,
    pub android: Mutex<AndroidSession>,
    pub pairing: Mutex<Option<tokio::task::AbortHandle>>,
    pub uxplay_log: LogTail,
    pub scrcpy_log: LogTail,
}

pub type State = Arc<Shared>;

impl Shared {
    pub fn new(app: AppHandle) -> State {
        let paths = Paths::resolve(&app);
        let _ = std::fs::create_dir_all(&paths.data);
        let settings: Settings = load_json(&paths.data.join("settings.json"));
        let devices = DeviceStore::open(paths.data.join("devices.json"));
        Arc::new(Self {
            app,
            settings: Mutex::new(settings),
            devices: Mutex::new(devices),
            discovery: MdnsDiscovery::default(),
            iphone: Mutex::new(Receiver::default()),
            android: Mutex::new(AndroidSession::default()),
            pairing: Mutex::new(None),
            uxplay_log: LogTail::default(),
            scrcpy_log: LogTail::default(),
            paths,
        })
    }

    pub fn adb(&self) -> AdbRunner {
        AdbRunner {
            adb: self.paths.adb.clone(),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn save_settings(&self, s: Settings) {
        let _ = save_json(&self.paths.data.join("settings.json"), &s);
        *self.settings.lock().unwrap() = s;
    }

    pub fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) {
        let _ = self.app.emit(event, payload);
    }

    pub fn toast(&self, kind: &str, message: impl Into<String>, path: Option<String>) {
        self.emit(
            "toast",
            serde_json::json!({ "kind": kind, "message": message.into(), "path": path }),
        );
    }

    pub fn screenshots_dir(&self) -> PathBuf {
        self.settings()
            .screenshots_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| self.paths.default_screenshots.clone())
    }

    pub fn recordings_dir(&self) -> PathBuf {
        self.settings()
            .recordings_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| self.paths.default_recordings.clone())
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// "2026-11-05 14-03-22" in local time, safe for file names.
pub fn stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H-%M-%S").to_string()
}

pub fn pc_name() -> String {
    gethostname::gethostname()
        .to_string_lossy()
        .trim()
        .to_string()
}
