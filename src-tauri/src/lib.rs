//! PhoneMirror desktop app: Tauri commands and lifecycle.
//!
//! Privacy: PhoneMirror has no telemetry and makes no internet requests. The
//! only network traffic is between this PC and phones on the local network.

mod android;
mod firewall;
mod iphone;
mod mdns;
mod proc;
mod state;
mod win;

use phonemirror_core::scrcpy::Source;
use phonemirror_core::settings::Settings;
use phonemirror_core::store::RememberedDevice;
use phonemirror_core::uxplay::receiver_name;
use phonemirror_core::Problem;
use serde::Serialize;
use state::{Shared, State};
use tauri::{Manager, RunEvent};
use tauri_plugin_opener::OpenerExt;

type Res<T> = Result<T, Problem>;

#[derive(Serialize)]
struct AppInfo {
    version: &'static str,
    os: &'static str,
    pc_name: String,
    receiver_name: String,
    engines: Engines,
    screenshots_dir: String,
    recordings_dir: String,
}

#[derive(Serialize)]
struct Engines {
    android: bool,
    iphone: bool,
}

#[tauri::command]
fn app_info(st: tauri::State<'_, State>) -> AppInfo {
    let s = st.settings();
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        pc_name: state::pc_name(),
        receiver_name: receiver_name(&state::pc_name(), s.receiver_name.as_deref()),
        engines: Engines {
            android: st.paths.adb.exists() && st.paths.scrcpy.exists(),
            iphone: st.paths.uxplay.exists(),
        },
        screenshots_dir: st.screenshots_dir().display().to_string(),
        recordings_dir: st.recordings_dir().display().to_string(),
    }
}

#[tauri::command]
fn get_settings(st: tauri::State<'_, State>) -> Settings {
    st.settings()
}

#[tauri::command]
async fn save_settings(st: tauri::State<'_, State>, settings: Settings) -> Res<Settings> {
    let old = st.settings();
    st.save_settings(settings.clone());
    let iphone_changed = old.receiver_name != settings.receiver_name
        || old.iphone_video_output != settings.iphone_video_output
        || old.iphone_video_casting != settings.iphone_video_casting
        || old.iphone_require_pin != settings.iphone_require_pin
        || old.iphone_quality != settings.iphone_quality;
    let st = st.inner().clone();
    if !settings.start_iphone_receiver && old.start_iphone_receiver {
        iphone::stop(&st);
    } else if settings.start_iphone_receiver && (!old.start_iphone_receiver || iphone_changed) {
        if iphone::status(&st).state == "connected" {
            st.toast(
                "info",
                "iPhone settings will apply the next time you start Screen Mirroring.",
                None,
            );
        } else {
            iphone::restart(st.clone()).await;
        }
    }
    Ok(st.settings())
}

// ----- iPhone -----

#[tauri::command]
fn iphone_status(st: tauri::State<'_, State>) -> iphone::IphoneStatus {
    iphone::status(&st)
}

#[tauri::command]
async fn iphone_action(
    st: tauri::State<'_, State>,
    action: String,
    value: serde_json::Value,
) -> Res<iphone::IphoneStatus> {
    iphone::action(st.inner().clone(), &action, value).await
}

#[tauri::command]
async fn firewall_status(st: tauri::State<'_, State>) -> Res<firewall::FirewallStatus> {
    Ok(firewall::status(&st).await)
}

#[tauri::command]
async fn firewall_fix(
    st: tauri::State<'_, State>,
    include_public: bool,
) -> Res<firewall::FirewallStatus> {
    firewall::fix(&st, include_public).await
}

// ----- Android -----

#[tauri::command]
async fn android_overview(st: tauri::State<'_, State>) -> Res<android::Overview> {
    android::overview(&st).await
}

#[tauri::command]
fn android_status(st: tauri::State<'_, State>) -> android::AndroidStatus {
    android::status(&st)
}

// async so it runs on the Tokio runtime (it spawns the pairing task).
#[tauri::command]
async fn android_start_pairing(st: tauri::State<'_, State>) -> Res<android::PairingStart> {
    Ok(android::start_qr_pairing(&st))
}

#[tauri::command]
fn android_cancel_pairing(st: tauri::State<'_, State>) {
    android::cancel_pairing(&st)
}

#[tauri::command]
async fn android_pair_code(
    st: tauri::State<'_, State>,
    address: String,
    code: String,
) -> Res<RememberedDevice> {
    android::pair_with_code(&st, &address, &code)
        .await
        .map(|c| c.device)
}

#[tauri::command]
async fn android_connect(st: tauri::State<'_, State>, id: String) -> Res<android::AndroidStatus> {
    let c = android::connect_remembered(&st, &id).await?;
    android::mirror(&st, &c.serial, None).await
}

#[tauri::command]
async fn android_mirror(
    st: tauri::State<'_, State>,
    serial: String,
    source: Option<Source>,
) -> Res<android::AndroidStatus> {
    android::mirror(&st, &serial, source).await
}

#[tauri::command]
async fn android_forget(st: tauri::State<'_, State>, id: String) -> Res<()> {
    android::forget(&st, &id).await
}

#[tauri::command]
async fn android_action(
    st: tauri::State<'_, State>,
    action: String,
    value: serde_json::Value,
) -> Res<android::AndroidStatus> {
    android::action(st.inner().clone(), &action, value).await
}

#[tauri::command]
async fn android_send_files(
    st: tauri::State<'_, State>,
    paths: Vec<String>,
    serial: Option<String>,
) -> Res<Vec<android::SendResult>> {
    android::send_files(&st, paths, serial).await
}

// ----- Misc -----

#[tauri::command]
fn reveal(app: tauri::AppHandle, path: String) -> Res<()> {
    let p = std::path::PathBuf::from(&path);
    let r = if p.is_dir() {
        app.opener().open_path(path, None::<&str>)
    } else {
        app.opener().reveal_item_in_dir(p)
    };
    r.map_err(|e| Problem::internal(e.to_string()))
}

#[tauri::command]
fn open_link(app: tauri::AppHandle, url: String) -> Res<()> {
    if !url.starts_with("https://") {
        return Err(Problem::internal("only https links"));
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| Problem::internal(e.to_string()))
}

#[tauri::command]
async fn diagnostics(st: tauri::State<'_, State>) -> Res<String> {
    let ifaces = mdns::local_ifaces();
    let fw = firewall::status(&st).await;
    let devices = android::overview(&st)
        .await
        .map(|o| format!("{:#?}", o.connected))
        .unwrap_or_else(|e| e.to_string());
    Ok(format!(
        "PhoneMirror {} on {} ({})\nPC name: {}\n\nEngines:\n{:#?}\n\nNetwork interfaces:\n{:#?}\n\nFirewall:\n{:#?}\n\niPhone status:\n{:#?}\n\nAndroid status:\n{:#?}\n\nadb devices:\n{}\n\n--- UxPlay log ---\n{}\n\n--- scrcpy log ---\n{}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        state::pc_name(),
        st.paths,
        ifaces,
        fw,
        iphone::status(&st),
        android::status(&st),
        devices,
        st.uxplay_log.dump(),
        st.scrcpy_log.dump(),
    ))
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // A second launch just brings the existing window forward.
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let st = Shared::new(app.handle().clone());
            app.manage(st.clone());
            if st.settings().start_iphone_receiver {
                // Engines are supervised by Tokio tasks; start inside the runtime.
                let st = st.clone();
                tauri::async_runtime::spawn(async move { iphone::start(&st) });
            } else {
                st.iphone.lock().unwrap().status.state = "off";
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            get_settings,
            save_settings,
            iphone_status,
            iphone_action,
            firewall_status,
            firewall_fix,
            android_overview,
            android_status,
            android_start_pairing,
            android_cancel_pairing,
            android_pair_code,
            android_connect,
            android_mirror,
            android_forget,
            android_action,
            android_send_files,
            reveal,
            open_link,
            diagnostics,
        ])
        .build(tauri::generate_context!())
        .expect("error while building PhoneMirror");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            if let Some(st) = handle.try_state::<State>() {
                android::cancel_pairing(&st);
                android::stop(&st);
                iphone::stop(&st);
                st.discovery.shutdown();
                // Give scrcpy a moment to finish an MP4 file.
                std::thread::sleep(std::time::Duration::from_millis(600));
            }
        }
    });
}
