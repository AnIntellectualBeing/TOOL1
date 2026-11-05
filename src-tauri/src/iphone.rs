//! The iPhone receiver: runs the bundled UxPlay, follows its log, restarts it
//! if it crashes, and exposes the toolbar actions.

use crate::proc::{self, StopStyle, Supervised};
use crate::state::{stamp, State};
use crate::win;
use phonemirror_core::settings::{normalize_rotation, Quality};
use phonemirror_core::uxplay::{receiver_name, UxPlayEvent, UxPlayMonitor, UxPlayOptions};
use phonemirror_core::Problem;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Default)]
pub struct Client {
    pub name: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct IphoneStatus {
    /// off | starting | waiting | connected | error
    pub state: &'static str,
    pub receiver_name: String,
    pub client: Option<Client>,
    pub mirroring: bool,
    pub pin: Option<String>,
    pub problem: Option<Problem>,
    /// This session is being recorded.
    pub recording: bool,
    /// The next session will be recorded.
    pub record_next: bool,
    pub speaker_only: bool,
    pub muted: bool,
    pub fullscreen: bool,
    pub topmost: bool,
    /// A changed setting will apply after this session ends.
    pub pending_restart: bool,
}

#[derive(Default)]
pub struct Receiver {
    child: Option<Supervised>,
    generation: u64,
    alive: bool,
    monitor: UxPlayMonitor,
    crashes: Vec<Instant>,
    record_base: Option<PathBuf>,
    pub status: IphoneStatus,
}

fn emit(st: &State) {
    let s = st.iphone.lock().unwrap().status.clone();
    st.emit("iphone-status", s);
}

pub fn status(st: &State) -> IphoneStatus {
    st.iphone.lock().unwrap().status.clone()
}

fn options(st: &State, r: &Receiver) -> UxPlayOptions {
    let s = st.settings();
    let mut o = UxPlayOptions::new(
        receiver_name(&crate::state::pc_name(), s.receiver_name.as_deref()),
        cfg!(windows),
    );
    o.quality = s.iphone_quality;
    o.rotation = s.iphone_rotation;
    o.video_output = s.iphone_video_output;
    o.video_casting = s.iphone_video_casting;
    o.require_pin = s.iphone_require_pin;
    o.audio_only = r.status.speaker_only;
    o.key_file = Some(st.paths.data.join("uxplay-key.pem").display().to_string());
    o.record_base = r.record_base.as_ref().map(|p| p.display().to_string());
    o
}

fn command(st: &State, opts: &UxPlayOptions) -> tokio::process::Command {
    let mut c = proc::command(&st.paths.uxplay);
    c.args(opts.args());
    let data = &st.paths.data;
    // Ignore any personal ~/.uxplayrc: PhoneMirror passes everything explicitly.
    let rc = data.join("uxplayrc");
    if !rc.exists() {
        let _ = std::fs::write(&rc, "# PhoneMirror passes options on the command line.\n");
    }
    c.env("UXPLAYRC", &rc).current_dir(data);
    if let Some(root) = &st.paths.gst_root {
        // Use only the bundled GStreamer, never one installed elsewhere.
        let plugins = root.join("lib").join("gstreamer-1.0");
        c.env("GST_PLUGIN_SYSTEM_PATH_1_0", &plugins)
            .env("GST_PLUGIN_SYSTEM_PATH", &plugins)
            .env("GST_PLUGIN_PATH_1_0", "")
            .env("GST_PLUGIN_PATH", "")
            .env("GST_REGISTRY_1_0", data.join("gstreamer-registry.bin"))
            .env("GST_REGISTRY", data.join("gstreamer-registry.bin"));
        let scanner = root.join("libexec").join("gstreamer-1.0").join(format!(
            "gst-plugin-scanner{}",
            std::env::consts::EXE_SUFFIX
        ));
        if scanner.exists() {
            c.env("GST_PLUGIN_SCANNER_1_0", &scanner)
                .env("GST_PLUGIN_SCANNER", &scanner);
        }
        let gio = root.join("lib").join("gio").join("modules");
        if gio.is_dir() {
            c.env("GIO_MODULE_DIR", gio);
        }
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut dirs = vec![root.clone()];
        dirs.extend(std::env::split_paths(&path));
        if let Ok(joined) = std::env::join_paths(dirs) {
            c.env("PATH", joined);
        }
    }
    c
}

/// Start the receiver if it isn't running.
pub fn start(st: &State) {
    let mut r = st.iphone.lock().unwrap();
    if r.child.is_some() {
        return;
    }
    let opts = options(st, &r);
    r.status.receiver_name = opts.name.clone();
    if st.paths.uxplay.is_absolute() && !st.paths.uxplay.exists() {
        r.status.state = "error";
        r.status.problem = Some(Problem::engine_missing(
            "iPhone",
            &st.paths.uxplay.display().to_string(),
        ));
        drop(r);
        emit(st);
        return;
    }
    r.generation += 1;
    let generation = r.generation;
    r.monitor = UxPlayMonitor::default();
    let st_line = st.clone();
    let st_exit = st.clone();
    log::info!("starting uxplay {:?}", opts.args());
    st.uxplay_log
        .push(&format!("$ uxplay {}", opts.args().join(" ")));
    let spawned = proc::spawn_supervised(
        command(st, &opts),
        StopStyle::Kill,
        Arc::new(move |line| on_line(&st_line, generation, &line)),
        Box::new(move |code, stopped| on_exit(&st_exit, generation, code, stopped)),
    );
    match spawned {
        Ok(child) => {
            r.child = Some(child);
            r.alive = true;
            r.status.state = "waiting";
            r.status.client = None;
            r.status.mirroring = false;
            r.status.pin = None;
            r.status.fullscreen = false;
            r.status.pending_restart = false;
            r.status.recording = false;
            if !matches!(
                r.status.problem.as_ref().map(|p| p.code),
                Some("receiver_crashed")
            ) {
                r.status.problem = None;
            }
        }
        Err(e) => {
            r.status.state = "error";
            r.status.problem = Some(Problem::engine_missing(
                "iPhone",
                &format!("{}: {e}", st.paths.uxplay.display()),
            ));
        }
    }
    drop(r);
    emit(st);
}

pub fn stop(st: &State) {
    let mut r = st.iphone.lock().unwrap();
    if let Some(mut c) = r.child.take() {
        c.stop();
    }
    r.status.state = "off";
    r.status.client = None;
    r.status.mirroring = false;
    drop(r);
    emit(st);
}

/// Stop, wait for the old process to free its ports, start again.
pub async fn restart(st: State) {
    stop(&st);
    for _ in 0..50 {
        if !st.iphone.lock().unwrap().alive {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    start(&st);
}

fn on_line(st: &State, generation: u64, line: &str) {
    st.uxplay_log.push(line);
    let mut r = st.iphone.lock().unwrap();
    if r.generation != generation {
        return;
    }
    let events = r.monitor.feed(line);
    if events.is_empty() {
        return;
    }
    let mut after_connect = false;
    let mut finished_recording = false;
    let mut restart_now = false;
    for ev in events {
        match ev {
            UxPlayEvent::ClientConnected { name, model } => {
                r.status.state = "connected";
                r.status.client = Some(Client { name, model });
                r.status.pin = None;
                r.status.problem = None;
                r.status.recording = r.record_base.is_some();
                after_connect = true;
            }
            UxPlayEvent::Mode { mirroring } => {
                r.status.mirroring = mirroring;
                after_connect = true;
            }
            UxPlayEvent::ClientDisconnected => {
                finished_recording = r.status.recording;
                r.status.state = "waiting";
                r.status.client = None;
                r.status.mirroring = false;
                r.status.fullscreen = false;
                r.status.recording = false;
                restart_now = r.status.pending_restart;
            }
            UxPlayEvent::Pin(pin) => r.status.pin = Some(pin),
            UxPlayEvent::CastingStarted => r.status.mirroring = true,
            UxPlayEvent::Problem { problem, fatal } => {
                if fatal {
                    r.status.state = "error";
                }
                r.status.problem = Some(problem);
            }
        }
    }
    let pid = r.child.as_ref().map(|c| c.pid).unwrap_or(0);
    let (topmost, muted) = (r.status.topmost, r.status.muted);
    drop(r);
    emit(st);

    if after_connect && (topmost || muted) {
        // The picture window and audio session appear a moment after connecting.
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1500)).await;
            if topmost {
                win::set_topmost(pid, true);
            }
            if muted {
                let _ = tokio::task::spawn_blocking(move || win::set_muted(pid, true)).await;
            }
        });
    }
    if finished_recording {
        let dir = st.recordings_dir();
        st.toast(
            "success",
            "iPhone recording saved.",
            Some(newest_mp4(&dir).unwrap_or(dir).display().to_string()),
        );
        // Don't record the next session unless asked again.
        let mut r = st.iphone.lock().unwrap();
        r.record_base = None;
        r.status.record_next = false;
        restart_now = true;
    }
    if restart_now {
        tokio::spawn(restart(st.clone()));
    }
}

fn newest_mp4(dir: &std::path::Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("mp4"))
        })
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
}

fn on_exit(st: &State, generation: u64, code: Option<i32>, stopped: bool) {
    let mut r = st.iphone.lock().unwrap();
    if r.generation != generation {
        return;
    }
    r.alive = false;
    r.child = None;
    if stopped {
        return;
    }
    let now = Instant::now();
    r.crashes
        .retain(|t| now.duration_since(*t) < Duration::from_secs(60));
    r.crashes.push(now);
    let details = format!("exit code {code:?}\n{}", st.uxplay_log.dump());
    let fatal_problem = matches!(r.status.state, "error");
    r.status.client = None;
    r.status.mirroring = false;
    if r.crashes.len() <= 3 && !fatal_problem {
        r.status.state = "starting";
        r.status.problem = Some(Problem::receiver_crashed(&details));
        drop(r);
        emit(st);
        let st = st.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1500)).await;
            start(&st);
        });
    } else {
        r.status.state = "error";
        if r.status.problem.is_none() || !fatal_problem {
            r.status.problem = Some(
                Problem::new("receiver_stopped", "The iPhone receiver keeps stopping.")
                    .hint("Press \"Restart receiver\". If it keeps happening, use \"Copy diagnostics\" and report it.")
                    .details(details),
            );
        }
        drop(r);
        emit(st);
    }
}

/// Apply a change now if no iPhone is connected, or after it disconnects.
fn apply_or_defer(st: &State, what: &str) {
    let mut r = st.iphone.lock().unwrap();
    if r.status.state == "connected" {
        r.status.pending_restart = true;
        drop(r);
        emit(st);
        st.toast(
            "info",
            format!("{what} will apply the next time you start Screen Mirroring."),
            None,
        );
    } else {
        drop(r);
        tokio::spawn(restart(st.clone()));
    }
}

fn pid(st: &State) -> Result<u32, Problem> {
    st.iphone
        .lock()
        .unwrap()
        .child
        .as_ref()
        .map(|c| c.pid)
        .ok_or_else(|| Problem::new("not_running", "The iPhone receiver isn't running."))
}

fn no_window() -> Problem {
    Problem::new("no_window", "The iPhone picture isn't showing yet.")
        .hint("Start Screen Mirroring on the iPhone first.")
}

pub async fn action(
    st: State,
    action: &str,
    value: serde_json::Value,
) -> Result<IphoneStatus, Problem> {
    let on = value.as_bool().unwrap_or(false);
    match action {
        "restart" => restart(st.clone()).await,
        "fullscreen" => {
            let pid = pid(&st)?;
            match win::toggle_fullscreen(pid) {
                Some(state) => st.iphone.lock().unwrap().status.fullscreen = state,
                None if cfg!(windows) => return Err(no_window()),
                None => {
                    return Err(Problem::new(
                        "unsupported",
                        "Use the picture window's own full-screen key.",
                    )
                    .hint("Click the iPhone picture and press F11 or Alt+Enter."))
                }
            }
        }
        "topmost" => {
            let pid = pid(&st)?;
            st.iphone.lock().unwrap().status.topmost = on;
            // If the picture window isn't open yet, this is applied when it appears.
            win::set_topmost(pid, on);
        }
        "mute" => {
            let pid = pid(&st)?;
            st.iphone.lock().unwrap().status.muted = on;
            match tokio::task::spawn_blocking(move || win::set_muted(pid, on)).await {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    return Err(Problem::new(
                        "unsupported",
                        "Sound can't be switched off from here on this system.",
                    )
                    .details(e))
                }
                Err(e) => return Err(Problem::internal(e.to_string())),
            }
        }
        "screenshot" => {
            let pid = pid(&st)?;
            let png = tokio::task::spawn_blocking(move || win::capture_window_png(pid))
                .await
                .map_err(|e| Problem::internal(e.to_string()))?
                .map_err(|e| no_window().details(e))?;
            let path = save_file(
                &st.screenshots_dir(),
                &format!("iPhone {}.png", stamp()),
                &png,
            )?;
            st.toast(
                "success",
                "Screenshot saved.",
                Some(path.display().to_string()),
            );
        }
        "record" => {
            let connected = st.iphone.lock().unwrap().status.state == "connected";
            if on {
                let dir = st.recordings_dir();
                std::fs::create_dir_all(&dir).map_err(|e| Problem::internal(e.to_string()))?;
                let mut r = st.iphone.lock().unwrap();
                r.record_base = Some(dir.join(format!("iPhone {}", stamp())));
                r.status.record_next = true;
            } else {
                let mut r = st.iphone.lock().unwrap();
                r.record_base = None;
                r.status.record_next = false;
            }
            if connected {
                let recording = st.iphone.lock().unwrap().status.recording;
                if on && !recording {
                    st.iphone.lock().unwrap().status.pending_restart = true;
                    st.toast(
                        "info",
                        "Recording starts with the next mirroring session. On the iPhone, stop Screen Mirroring and start it again to begin now.",
                        None,
                    );
                } else if !on && recording {
                    st.toast("info", "To finish the recording, stop Screen Mirroring on the iPhone. The file is saved then.", None);
                }
            } else {
                restart(st.clone()).await;
                if on {
                    st.toast(
                        "info",
                        "Ready to record. Recording starts when your iPhone connects.",
                        None,
                    );
                }
            }
        }
        "rotate" => {
            let mut s = st.settings();
            s.iphone_rotation = normalize_rotation(s.iphone_rotation as i32 + 90);
            st.save_settings(s);
            apply_or_defer(&st, "Rotation");
        }
        "quality" => {
            let q: Quality =
                serde_json::from_value(value).map_err(|e| Problem::internal(e.to_string()))?;
            let mut s = st.settings();
            s.iphone_quality = q;
            st.save_settings(s);
            apply_or_defer(&st, "The new quality");
        }
        "speaker_only" => {
            st.iphone.lock().unwrap().status.speaker_only = on;
            apply_or_defer(
                &st,
                if on {
                    "Speaker-only mode"
                } else {
                    "Video mode"
                },
            );
        }
        other => return Err(Problem::internal(format!("unknown action {other}"))),
    }
    emit(&st);
    Ok(status(&st))
}

pub fn save_file(dir: &std::path::Path, name: &str, data: &[u8]) -> Result<PathBuf, Problem> {
    std::fs::create_dir_all(dir).map_err(|e| {
        Problem::new("save_failed", "The file couldn't be saved.")
            .hint("Choose another folder in Settings.")
            .details(e.to_string())
    })?;
    let path = dir.join(name);
    std::fs::write(&path, data).map_err(|e| {
        Problem::new("save_failed", "The file couldn't be saved.")
            .hint("Choose another folder in Settings.")
            .details(e.to_string())
    })?;
    Ok(path)
}
