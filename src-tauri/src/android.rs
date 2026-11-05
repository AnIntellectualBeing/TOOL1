//! Android: devices, wireless pairing, and the scrcpy mirroring session.

use crate::iphone::save_file;
use crate::mdns::local_ifaces;
use crate::proc::{self, StopStyle, Supervised};
use crate::state::{now, stamp, State};
use crate::win;
use phonemirror_core::adb::{Adb, AdbDevice, DeviceState, Transport};
use phonemirror_core::pairing::{self, Connected, PairingStep, Timeouts};
use phonemirror_core::qr::PairingCredentials;
use phonemirror_core::scrcpy::{
    classify_exit, classify_line, ExitKind, ScrcpyEvent, ScrcpyOptions, Source,
};
use phonemirror_core::settings::{normalize_rotation, Quality};
use phonemirror_core::store::RememberedDevice;
use phonemirror_core::Problem;
use serde::Serialize;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Default)]
pub struct AndroidStatus {
    /// idle | starting | mirroring | reconnecting | error
    pub state: &'static str,
    pub serial: Option<String>,
    pub name: Option<String>,
    pub wireless: bool,
    pub camera: bool,
    pub problem: Option<Problem>,
    /// Non-fatal notes (e.g. "sound needs Android 11").
    pub warnings: Vec<Problem>,
    pub attempt: u32,
    pub recording: Option<String>,
    pub muted: bool,
    pub fullscreen: bool,
    pub topmost: bool,
    pub screen_off: bool,
    pub audio: bool,
    pub quality: Quality,
    pub rotation: u16,
}

#[derive(Default)]
pub struct AndroidSession {
    child: Option<Supervised>,
    generation: u64,
    opts: Option<ScrcpyOptions>,
    device_id: Option<String>,
    started: Option<Instant>,
    fatal_seen: bool,
    reconnect: Option<tokio::task::AbortHandle>,
    pub status: AndroidStatus,
}

fn emit(st: &State) {
    let s = st.android.lock().unwrap().status.clone();
    st.emit("android-status", s);
}

pub fn status(st: &State) -> AndroidStatus {
    let mut s = st.android.lock().unwrap().status.clone();
    if s.state.is_empty() {
        s.state = "idle";
    }
    s
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceRow {
    pub serial: String,
    pub state: DeviceState,
    pub transport: Transport,
    pub name: String,
    pub problem: Option<Problem>,
    pub remembered_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    pub connected: Vec<DeviceRow>,
    pub remembered: Vec<RememberedDevice>,
}

fn remembered_for(list: &[RememberedDevice], d: &AdbDevice) -> Option<RememberedDevice> {
    list.iter()
        .find(|r| {
            d.mdns_instance().is_some_and(|m| {
                r.mdns_instance
                    .as_deref()
                    .is_some_and(|x| x.eq_ignore_ascii_case(m))
            }) || r.last_address.as_deref() == Some(d.serial.as_str())
                || r.usb_serial.as_deref() == Some(d.serial.as_str())
        })
        .cloned()
}

pub async fn overview(st: &State) -> Result<Overview, Problem> {
    let runner = st.adb();
    let devices = Adb::new(&runner).devices().await?;
    let remembered = st.devices.lock().unwrap().all();
    let connected = devices
        .iter()
        .filter(|d| d.transport != Transport::Emulator || d.state == DeviceState::Device)
        .map(|d| {
            let rem = remembered_for(&remembered, d);
            DeviceRow {
                serial: d.serial.clone(),
                state: d.state,
                transport: d.transport,
                name: rem
                    .as_ref()
                    .map(|r| r.name.clone())
                    .or_else(|| d.display_model())
                    .unwrap_or_else(|| "Android phone".into()),
                problem: match d.state {
                    DeviceState::Unauthorized | DeviceState::Authorizing => {
                        Some(Problem::unauthorized())
                    }
                    DeviceState::Offline => Some(Problem::device_offline()),
                    DeviceState::NoPermissions => Some(Problem::no_usb_permission()),
                    _ => None,
                },
                remembered_id: rem.map(|r| r.id),
            }
        })
        .collect();
    Ok(Overview {
        connected,
        remembered,
    })
}

fn timeouts() -> Timeouts {
    Timeouts::default()
}

// ----- Pairing -----

#[derive(Debug, Clone, Serialize)]
pub struct PairingStart {
    pub svg: String,
    pub service_name: String,
}

fn remember(st: &State, c: &Connected) {
    let _ = st.devices.lock().unwrap().upsert(c.device.clone());
}

pub fn start_qr_pairing(st: &State) -> PairingStart {
    cancel_pairing(st);
    let creds = PairingCredentials::generate();
    let out = PairingStart {
        svg: creds.qr_svg(),
        service_name: creds.service_name.clone(),
    };
    let st2 = st.clone();
    let task = tokio::spawn(async move {
        let runner = st2.adb();
        let ifaces = local_ifaces();
        let progress_st = st2.clone();
        let progress = move |step: PairingStep| {
            progress_st.emit(
                "pairing",
                serde_json::json!({ "type": "step", "step": step }),
            );
        };
        let result = pairing::pair_with_qr(
            &runner,
            &st2.discovery,
            &creds,
            &ifaces,
            &timeouts(),
            now(),
            &progress,
        )
        .await;
        finish_pairing(&st2, result);
    });
    *st.pairing.lock().unwrap() = Some(task.abort_handle());
    out
}

fn finish_pairing(st: &State, result: Result<Connected, Problem>) {
    match result {
        Ok(c) => {
            remember(st, &c);
            st.emit(
                "pairing",
                serde_json::json!({ "type": "done", "serial": c.serial, "device": c.device }),
            );
        }
        Err(problem) => st.emit(
            "pairing",
            serde_json::json!({ "type": "error", "problem": problem }),
        ),
    }
}

pub fn cancel_pairing(st: &State) {
    if let Some(h) = st.pairing.lock().unwrap().take() {
        h.abort();
    }
}

pub async fn pair_with_code(st: &State, address: &str, code: &str) -> Result<Connected, Problem> {
    let runner = st.adb();
    let ifaces = local_ifaces();
    let st2 = st.clone();
    let progress = move |step: PairingStep| {
        st2.emit(
            "pairing",
            serde_json::json!({ "type": "step", "step": step }),
        )
    };
    let c = pairing::pair_with_code(
        &runner,
        &st.discovery,
        address,
        code,
        &ifaces,
        &timeouts(),
        now(),
        &progress,
    )
    .await?;
    remember(st, &c);
    Ok(c)
}

pub async fn connect_remembered(st: &State, id: &str) -> Result<Connected, Problem> {
    let device = st
        .devices
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .ok_or_else(|| Problem::internal(format!("unknown device {id}")))?;
    let runner = st.adb();
    let ifaces = local_ifaces();
    let st2 = st.clone();
    let progress = move |step: PairingStep| {
        st2.emit(
            "pairing",
            serde_json::json!({ "type": "step", "step": step }),
        )
    };
    let c = pairing::connect_remembered(
        &runner,
        &st.discovery,
        &device,
        &ifaces,
        &timeouts(),
        now(),
        &progress,
    )
    .await?;
    remember(st, &c);
    Ok(c)
}

pub async fn forget(st: &State, id: &str) -> Result<(), Problem> {
    let removed = st.devices.lock().unwrap().get(id).cloned();
    if let Some(d) = removed {
        if let Some(addr) = d.last_address {
            let runner = st.adb();
            let _ = Adb::new(&runner).disconnect(&addr).await;
        }
    }
    st.devices
        .lock()
        .unwrap()
        .remove(id)
        .map_err(|e| Problem::internal(e.to_string()))?;
    Ok(())
}

// ----- Mirroring -----

fn base_options(st: &State, serial: &str, wireless: bool) -> ScrcpyOptions {
    let s = st.settings();
    let mut o = ScrcpyOptions::new(serial, wireless);
    o.quality = s.android_quality;
    o.audio = s.android_audio;
    o.screen_off = s.android_screen_off;
    o.stay_awake = s.android_stay_awake;
    o.clipboard_sync = s.android_clipboard_sync;
    o.always_on_top = s.always_on_top;
    o.rotation = s.android_rotation;
    o
}

/// Start mirroring a connected device.
pub async fn mirror(
    st: &State,
    serial: &str,
    source: Option<Source>,
) -> Result<AndroidStatus, Problem> {
    let ov = overview(st).await?;
    let row = ov
        .connected
        .iter()
        .find(|d| d.serial == serial)
        .ok_or_else(Problem::no_android_device)?;
    if let Some(p) = &row.problem {
        return Err(p.clone());
    }
    let mut opts = base_options(st, serial, row.transport == Transport::Wifi);
    opts.title = format!("{} – PhoneMirror", row.name);
    if let Some(src) = source {
        opts.source = src;
    }
    launch(st, opts, row.name.clone(), row.remembered_id.clone())?;
    Ok(status(st))
}

fn launch(
    st: &State,
    opts: ScrcpyOptions,
    name: String,
    device_id: Option<String>,
) -> Result<(), Problem> {
    if st.paths.scrcpy.is_absolute() && !st.paths.scrcpy.exists() {
        return Err(Problem::engine_missing(
            "Android",
            &st.paths.scrcpy.display().to_string(),
        ));
    }
    let mut a = st.android.lock().unwrap();
    if let Some(h) = a.reconnect.take() {
        h.abort();
    }
    if let Some(mut old) = a.child.take() {
        old.stop();
    }
    a.generation += 1;
    let generation = a.generation;
    let mut cmd = proc::command(&st.paths.scrcpy);
    cmd.args(opts.args()).env("ADB", &st.paths.adb);
    if let Some(dir) = st
        .paths
        .scrcpy
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
    {
        cmd.current_dir(dir);
    }
    st.scrcpy_log
        .push(&format!("$ scrcpy {}", opts.args().join(" ")));
    let st_line = st.clone();
    let st_exit = st.clone();
    let child = proc::spawn_supervised(
        cmd,
        StopStyle::CloseWindow,
        Arc::new(move |line| on_line(&st_line, generation, &line)),
        Box::new(move |code, stopped| on_exit(&st_exit, generation, code, stopped)),
    )
    .map_err(|e| {
        Problem::engine_missing("Android", &format!("{}: {e}", st.paths.scrcpy.display()))
    })?;
    a.child = Some(child);
    a.started = Some(Instant::now());
    a.fatal_seen = false;
    a.device_id = device_id;
    let keep_muted = a.status.muted && a.status.serial.as_deref() == Some(opts.serial.as_str());
    a.status = AndroidStatus {
        state: "starting",
        serial: Some(opts.serial.clone()),
        name: Some(name),
        wireless: opts.wireless,
        camera: matches!(opts.source, Source::Camera { .. }),
        recording: opts.record_to.clone(),
        topmost: opts.always_on_top,
        fullscreen: opts.fullscreen,
        screen_off: opts.screen_off,
        audio: opts.audio,
        quality: opts.quality,
        rotation: opts.rotation,
        muted: keep_muted,
        ..Default::default()
    };
    a.opts = Some(opts);
    drop(a);
    emit(st);
    Ok(())
}

fn on_line(st: &State, generation: u64, line: &str) {
    st.scrcpy_log.push(line);
    let Some(ev) = classify_line(line) else {
        return;
    };
    let mut a = st.android.lock().unwrap();
    if a.generation != generation {
        return;
    }
    let mut reapply_mute = None;
    match ev {
        ScrcpyEvent::Started => {
            a.status.state = "mirroring";
            a.status.attempt = 0;
            a.status.problem = None;
            if a.status.muted {
                reapply_mute = a.child.as_ref().map(|c| c.pid);
            }
        }
        ScrcpyEvent::RecordingTo(p) => a.status.recording = Some(p),
        ScrcpyEvent::RecordingSaved(p) => {
            drop(a);
            st.toast("success", "Recording saved.", Some(p));
            return;
        }
        ScrcpyEvent::Problem { problem, fatal } => {
            if fatal {
                a.fatal_seen = true;
                a.status.problem = Some(problem);
            } else if !a.status.warnings.iter().any(|w| w.code == problem.code) {
                a.status.warnings.push(problem);
            }
        }
    }
    drop(a);
    emit(st);
    if let Some(pid) = reapply_mute {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1500)).await;
            let _ = tokio::task::spawn_blocking(move || win::set_muted(pid, true)).await;
        });
    }
}

fn on_exit(st: &State, generation: u64, code: Option<i32>, stopped: bool) {
    let mut a = st.android.lock().unwrap();
    if a.generation != generation {
        return;
    }
    a.child = None;
    if stopped {
        a.status = AndroidStatus {
            state: "idle",
            ..Default::default()
        };
        drop(a);
        emit(st);
        return;
    }
    let kind = classify_exit(code);
    let ran_for = a.started.map(|t| t.elapsed()).unwrap_or_default();
    let was_mirroring = a.status.state == "mirroring" || ran_for > Duration::from_secs(5);
    let auto = st.settings().auto_reconnect;
    match kind {
        ExitKind::Closed => {
            a.status = AndroidStatus {
                state: "idle",
                ..Default::default()
            };
        }
        ExitKind::Disconnected | ExitKind::Crashed | ExitKind::StartFailed
            if auto && was_mirroring && !a.fatal_seen && a.opts.is_some() =>
        {
            a.status.state = "reconnecting";
            let st2 = st.clone();
            let task = tokio::spawn(async move { reconnect_loop(st2).await });
            a.reconnect = Some(task.abort_handle());
        }
        _ => {
            a.status.state = "error";
            if a.status.problem.is_none() {
                let tail = st.scrcpy_log.dump();
                a.status.problem = Some(
                    Problem::new("mirroring_failed", "Mirroring stopped.")
                        .hint(
                            "Check the cable or Wi-Fi, unlock the phone, and press Connect again.",
                        )
                        .details(format!("exit code {code:?}\n{tail}")),
                );
            }
        }
    }
    drop(a);
    emit(st);
}

async fn reconnect_loop(st: State) {
    let (opts, device_id, name) = {
        let a = st.android.lock().unwrap();
        (
            a.opts.clone(),
            a.device_id.clone(),
            a.status.name.clone().unwrap_or_else(|| "your phone".into()),
        )
    };
    let Some(mut opts) = opts else { return };
    let runner = st.adb();
    for (i, delay) in [2u64, 3, 5, 8, 13, 20].iter().enumerate() {
        {
            let mut a = st.android.lock().unwrap();
            a.status.state = "reconnecting";
            a.status.attempt = i as u32 + 1;
        }
        emit(&st);
        tokio::time::sleep(Duration::from_secs(*delay)).await;
        let serial = if let (true, Some(id)) = (opts.wireless, device_id.as_deref()) {
            connect_remembered(&st, id).await.ok().map(|c| c.serial)
        } else {
            Adb::new(&runner)
                .devices()
                .await
                .ok()
                .and_then(|d| {
                    d.into_iter()
                        .find(|d| d.serial == opts.serial && d.state == DeviceState::Device)
                })
                .map(|d| d.serial)
        };
        if let Some(serial) = serial {
            opts.serial = serial;
            // Don't overwrite an earlier recording.
            if opts.record_to.is_some() {
                opts.record_to = Some(recording_path(&st, &name));
            }
            if launch(&st, opts.clone(), name.clone(), device_id.clone()).is_ok() {
                return;
            }
        }
    }
    let mut a = st.android.lock().unwrap();
    a.status.state = "error";
    a.status.problem = Some(if opts.wireless {
        Problem::phone_not_visible(&name)
    } else {
        Problem::new("disconnected", format!("{name} was disconnected."))
            .hint("Plug the cable back in and press Connect.")
    });
    drop(a);
    emit(&st);
}

pub fn stop(st: &State) {
    let mut a = st.android.lock().unwrap();
    if let Some(h) = a.reconnect.take() {
        h.abort();
    }
    match a.child.take() {
        Some(mut c) => c.stop(),
        None => {
            a.status = AndroidStatus {
                state: "idle",
                ..Default::default()
            }
        }
    }
    drop(a);
    emit(st);
}

fn recording_path(st: &State, name: &str) -> String {
    let dir = st.recordings_dir();
    let _ = std::fs::create_dir_all(&dir);
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    dir.join(format!("{} {}.mp4", safe.trim(), stamp()))
        .display()
        .to_string()
}

/// Restart scrcpy with changed options.
fn relaunch(st: &State, change: impl FnOnce(&mut ScrcpyOptions)) -> Result<(), Problem> {
    let (opts, name, id) = {
        let a = st.android.lock().unwrap();
        (
            a.opts.clone(),
            a.status.name.clone().unwrap_or_default(),
            a.device_id.clone(),
        )
    };
    let mut opts =
        opts.ok_or_else(|| Problem::new("not_mirroring", "Start mirroring a phone first."))?;
    change(&mut opts);
    launch(st, opts, name, id)
}

fn active_pid(st: &State) -> Result<u32, Problem> {
    st.android
        .lock()
        .unwrap()
        .child
        .as_ref()
        .map(|c| c.pid)
        .ok_or_else(|| Problem::new("not_mirroring", "Start mirroring a phone first."))
}

fn active_serial(st: &State) -> Result<String, Problem> {
    st.android
        .lock()
        .unwrap()
        .status
        .serial
        .clone()
        .ok_or_else(|| Problem::new("not_mirroring", "Start mirroring a phone first."))
}

pub async fn action(
    st: State,
    action: &str,
    value: serde_json::Value,
) -> Result<AndroidStatus, Problem> {
    let on = value.as_bool().unwrap_or(false);
    match action {
        "fullscreen" => {
            let pid = active_pid(&st)?;
            match win::toggle_fullscreen(pid) {
                Some(state) => st.android.lock().unwrap().status.fullscreen = state,
                None => {
                    let fs = !st.android.lock().unwrap().status.fullscreen;
                    relaunch(&st, |o| o.fullscreen = fs)?;
                }
            }
        }
        "topmost" => {
            let pid = active_pid(&st)?;
            if win::set_topmost(pid, on) {
                let mut a = st.android.lock().unwrap();
                a.status.topmost = on;
                if let Some(o) = a.opts.as_mut() {
                    o.always_on_top = on;
                }
            } else {
                relaunch(&st, |o| o.always_on_top = on)?;
            }
        }
        "mute" => {
            let pid = active_pid(&st)?;
            match tokio::task::spawn_blocking(move || win::set_muted(pid, on)).await {
                Ok(Ok(_)) => st.android.lock().unwrap().status.muted = on,
                _ => relaunch(&st, |o| o.audio = !on)?,
            }
        }
        "audio" => {
            let mut s = st.settings();
            s.android_audio = on;
            st.save_settings(s);
            relaunch(&st, |o| o.audio = on)?;
        }
        "screenshot" => {
            let serial = active_serial(&st)?;
            let runner = st.adb();
            let png = Adb::new(&runner).screenshot_png(&serial).await?;
            let name = st
                .android
                .lock()
                .unwrap()
                .status
                .name
                .clone()
                .unwrap_or_else(|| "Android".into());
            let path = save_file(
                &st.screenshots_dir(),
                &format!("{name} {}.png", stamp()).replace(['/', '\\', ':'], "_"),
                &png,
            )?;
            st.toast(
                "success",
                "Screenshot saved.",
                Some(path.display().to_string()),
            );
        }
        "record" => {
            let name = st
                .android
                .lock()
                .unwrap()
                .status
                .name
                .clone()
                .unwrap_or_else(|| "Android".into());
            let path = on.then(|| recording_path(&st, &name));
            relaunch(&st, |o| o.record_to = path)?;
        }
        "rotate" => {
            let mut s = st.settings();
            s.android_rotation = normalize_rotation(s.android_rotation as i32 + 90);
            let r = s.android_rotation;
            st.save_settings(s);
            relaunch(&st, |o| o.rotation = r)?;
        }
        "quality" => {
            let q: Quality =
                serde_json::from_value(value).map_err(|e| Problem::internal(e.to_string()))?;
            let mut s = st.settings();
            s.android_quality = q;
            st.save_settings(s);
            relaunch(&st, |o| o.quality = q)?;
        }
        "screen_off" => {
            let mut s = st.settings();
            s.android_screen_off = on;
            st.save_settings(s);
            relaunch(&st, |o| o.screen_off = on)?;
        }
        "clipboard" => {
            let mut s = st.settings();
            s.android_clipboard_sync = on;
            st.save_settings(s);
            relaunch(&st, |o| o.clipboard_sync = on)?;
        }
        "source" => {
            let src: Source =
                serde_json::from_value(value).map_err(|e| Problem::internal(e.to_string()))?;
            relaunch(&st, |o| o.source = src)?;
        }
        "stop" => stop(&st),
        other => return Err(Problem::internal(format!("unknown action {other}"))),
    }
    emit(&st);
    Ok(status(&st))
}

#[derive(Debug, Clone, Serialize)]
pub struct SendResult {
    pub file: String,
    pub installed: bool,
    pub problem: Option<Problem>,
}

/// Dropped files: APKs are installed, everything else goes to Download.
pub async fn send_files(
    st: &State,
    paths: Vec<String>,
    serial: Option<String>,
) -> Result<Vec<SendResult>, Problem> {
    let serial = match serial {
        Some(s) => s,
        None => active_serial(st)?,
    };
    let runner = st.adb();
    let adb = Adb::new(&runner);
    let mut results = Vec::new();
    for p in paths {
        let file = std::path::Path::new(&p)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or(p.clone());
        let is_apk = p.to_lowercase().ends_with(".apk");
        if std::path::Path::new(&p).is_dir() {
            results.push(SendResult {
                file,
                installed: false,
                problem: Some(Problem::new(
                    "folder",
                    "Folders can't be sent yet. Drop the files inside it instead.",
                )),
            });
            continue;
        }
        st.toast(
            "info",
            if is_apk {
                format!("Installing {file}…")
            } else {
                format!("Sending {file}…")
            },
            None,
        );
        let r = if is_apk {
            adb.install(&serial, &p).await
        } else {
            adb.push(&serial, &p, "/sdcard/Download/").await
        };
        match &r {
            Ok(()) => st.toast(
                "success",
                if is_apk {
                    format!("{file} installed.")
                } else {
                    format!("{file} is in the phone's Download folder.")
                },
                None,
            ),
            Err(e) => st.toast("error", e.to_string(), None),
        }
        results.push(SendResult {
            file,
            installed: is_apk && r.is_ok(),
            problem: r.err(),
        });
    }
    Ok(results)
}
