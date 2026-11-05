//! scrcpy command lines and output interpretation.

use crate::errors::Problem;
use crate::settings::Quality;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraFacing {
    Back,
    Front,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    Screen,
    Camera { facing: CameraFacing },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrcpyOptions {
    pub serial: String,
    pub wireless: bool,
    pub title: String,
    pub quality: Quality,
    pub audio: bool,
    pub screen_off: bool,
    pub stay_awake: bool,
    pub clipboard_sync: bool,
    pub always_on_top: bool,
    pub fullscreen: bool,
    pub rotation: u16,
    pub record_to: Option<String>,
    pub source: Source,
}

/// (max size in px, 0 = native; video bit rate; max fps)
pub fn preset(quality: Quality, wireless: bool) -> (u32, &'static str, u32) {
    let q = match quality {
        Quality::Auto if wireless => Quality::Smooth,
        Quality::Auto => Quality::Balanced,
        q => q,
    };
    match q {
        Quality::Smooth => (1280, "4M", 60),
        Quality::Balanced => (1920, "8M", 60),
        Quality::Sharp => (0, "16M", 60),
        Quality::BatterySaver => (1024, "2M", 30),
        Quality::Auto => unreachable!(),
    }
}

impl ScrcpyOptions {
    pub fn new(serial: impl Into<String>, wireless: bool) -> Self {
        Self {
            serial: serial.into(),
            wireless,
            title: "PhoneMirror".into(),
            quality: Quality::Auto,
            audio: true,
            screen_off: false,
            stay_awake: true,
            clipboard_sync: true,
            always_on_top: false,
            fullscreen: false,
            rotation: 0,
            record_to: None,
            source: Source::Screen,
        }
    }

    pub fn args(&self) -> Vec<String> {
        let (max_size, bit_rate, fps) = preset(self.quality, self.wireless);
        let mut a = vec![
            format!("--serial={}", self.serial),
            format!("--window-title={}", self.title),
            // H.264 decodes fastest everywhere: lowest latency.
            "--video-codec=h264".to_string(),
            format!("--video-bit-rate={bit_rate}"),
            format!("--max-fps={fps}"),
            // Files dropped on the window land in the phone's Download folder.
            "--push-target=/sdcard/Download/".to_string(),
        ];
        if max_size > 0 {
            a.push(format!("--max-size={max_size}"));
        }
        if !self.audio {
            a.push("--no-audio".into());
        }
        match self.source {
            Source::Screen => {
                if self.screen_off {
                    a.push("--turn-screen-off".into());
                }
                if self.stay_awake {
                    a.push("--stay-awake".into());
                }
            }
            Source::Camera { facing } => {
                a.push("--video-source=camera".into());
                a.push(format!(
                    "--camera-facing={}",
                    match facing {
                        CameraFacing::Back => "back",
                        CameraFacing::Front => "front",
                    }
                ));
            }
        }
        if !self.clipboard_sync {
            a.push("--no-clipboard-autosync".into());
        }
        if self.always_on_top {
            a.push("--always-on-top".into());
        }
        if self.fullscreen {
            a.push("--fullscreen".into());
        }
        if self.rotation != 0 {
            a.push(format!("--orientation={}", self.rotation));
        }
        if let Some(path) = &self.record_to {
            a.push(format!("--record={path}"));
        }
        a
    }
}

/// Something worth telling the person, recognised in scrcpy's output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScrcpyEvent {
    /// The first frame is on screen.
    Started,
    RecordingTo(String),
    RecordingSaved(String),
    /// A problem; `fatal` means mirroring won't continue.
    Problem {
        problem: Problem,
        fatal: bool,
    },
}

pub fn classify_line(line: &str) -> Option<ScrcpyEvent> {
    let l = line.trim();
    let p = |problem: Problem, fatal: bool| {
        Some(ScrcpyEvent::Problem {
            problem: problem.details(l),
            fatal,
        })
    };
    if l.contains("Texture:") {
        return Some(ScrcpyEvent::Started);
    }
    if l.contains("Device is unauthorized") {
        return p(Problem::unauthorized(), true);
    }
    if l.contains("Could not find any ADB device") || l.contains("Could not find ADB device") {
        return p(Problem::no_android_device(), true);
    }
    if l.contains("not supported before Android 11") {
        return p(Problem::audio_needs_android_11(), false);
    }
    if l.contains("audio capture must be started in the foreground") {
        return p(Problem::audio_needs_unlock(), false);
    }
    if l.contains("INJECT_EVENTS permission") {
        return p(Problem::input_blocked_by_brand(), false);
    }
    if l.contains("Camera mirroring is not supported before Android 12") {
        return p(Problem::camera_needs_android_12(), true);
    }
    if l.contains("Could not open audio device") {
        return p(
            Problem::new("pc_audio", "Your PC's speakers couldn't be opened, so there's no sound.")
                .hint("Check that a speaker or headphones are connected and not used exclusively by another app."),
            false,
        );
    }
    if let Some(rest) = l.split("Recording started to ").nth(1) {
        return Some(ScrcpyEvent::RecordingTo(after_colon(rest)));
    }
    if let Some(rest) = l.split("Recording complete to ").nth(1) {
        return Some(ScrcpyEvent::RecordingSaved(after_colon(rest)));
    }
    None
}

/// "mp4 file: C:\x.mp4" → "C:\x.mp4"
fn after_colon(s: &str) -> String {
    s.split_once("file: ")
        .map(|(_, p)| p)
        .unwrap_or(s)
        .trim()
        .to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitKind {
    /// Window closed by the person (or by us).
    Closed,
    /// Couldn't start (bad device, missing permission...).
    StartFailed,
    /// Ran, then lost the phone (cable pulled, Wi-Fi dropped).
    Disconnected,
    Crashed,
}

/// scrcpy exit status: 0 = normal, 1 = start failure, 2 = device disconnected.
pub fn classify_exit(code: Option<i32>) -> ExitKind {
    match code {
        Some(0) => ExitKind::Closed,
        Some(1) => ExitKind::StartFailed,
        Some(2) => ExitKind::Disconnected,
        _ => ExitKind::Crashed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_args_wireless() {
        let a = ScrcpyOptions::new("192.168.1.5:4000", true).args();
        assert!(a.contains(&"--serial=192.168.1.5:4000".to_string()));
        assert!(a.contains(&"--max-size=1280".to_string()));
        assert!(a.contains(&"--video-bit-rate=4M".to_string()));
        assert!(a.contains(&"--video-codec=h264".to_string()));
        assert!(a.contains(&"--stay-awake".to_string()));
        assert!(!a.iter().any(|x| x == "--no-audio"));
    }

    #[test]
    fn usb_sharp_has_no_max_size() {
        let mut o = ScrcpyOptions::new("R58M", false);
        o.quality = Quality::Sharp;
        let a = o.args();
        assert!(!a.iter().any(|x| x.starts_with("--max-size")));
        assert!(a.contains(&"--video-bit-rate=16M".to_string()));
        let mut o = ScrcpyOptions::new("R58M", false);
        o.quality = Quality::Auto;
        assert!(o.args().contains(&"--max-size=1920".to_string()));
    }

    #[test]
    fn all_toggles() {
        let mut o = ScrcpyOptions::new("S", false);
        o.audio = false;
        o.screen_off = true;
        o.clipboard_sync = false;
        o.always_on_top = true;
        o.fullscreen = true;
        o.rotation = 90;
        o.record_to = Some("C:\\Videos\\a.mp4".into());
        let a = o.args();
        for want in [
            "--no-audio",
            "--turn-screen-off",
            "--no-clipboard-autosync",
            "--always-on-top",
            "--fullscreen",
            "--orientation=90",
            "--record=C:\\Videos\\a.mp4",
        ] {
            assert!(a.contains(&want.to_string()), "missing {want}");
        }
    }

    #[test]
    fn camera_mode() {
        let mut o = ScrcpyOptions::new("S", false);
        o.source = Source::Camera {
            facing: CameraFacing::Front,
        };
        o.screen_off = true;
        let a = o.args();
        assert!(a.contains(&"--video-source=camera".to_string()));
        assert!(a.contains(&"--camera-facing=front".to_string()));
        // Turning the screen off is meaningless (and rejected) for the camera.
        assert!(!a.contains(&"--turn-screen-off".to_string()));
    }

    #[test]
    fn classifies_output() {
        assert_eq!(
            classify_line("INFO: Texture: 1080x2400"),
            Some(ScrcpyEvent::Started)
        );
        let e = classify_line("ERROR: Device is unauthorized:").unwrap();
        assert!(
            matches!(e, ScrcpyEvent::Problem { ref problem, fatal: true } if problem.code == "unauthorized")
        );
        let e =
            classify_line("WARN: Audio disabled: it is not supported before Android 11").unwrap();
        assert!(
            matches!(e, ScrcpyEvent::Problem { ref problem, fatal: false } if problem.code == "audio_unsupported")
        );
        let e = classify_line("java.lang.SecurityException: Injecting input events requires the caller (or the source of the instrumentation, if any) to have the INJECT_EVENTS permission.").unwrap();
        assert!(
            matches!(e, ScrcpyEvent::Problem { ref problem, .. } if problem.code == "input_blocked")
        );
        assert_eq!(
            classify_line("INFO: Recording started to mp4 file: C:\\v\\a.mp4"),
            Some(ScrcpyEvent::RecordingTo("C:\\v\\a.mp4".into()))
        );
        assert_eq!(
            classify_line("INFO: Recording complete to mp4 file: C:\\v\\a.mp4"),
            Some(ScrcpyEvent::RecordingSaved("C:\\v\\a.mp4".into()))
        );
        let e =
            classify_line("[server] ERROR: Camera mirroring is not supported before Android 12")
                .unwrap();
        assert!(
            matches!(e, ScrcpyEvent::Problem { ref problem, fatal: true } if problem.code == "camera_unsupported")
        );
        assert_eq!(
            classify_line("INFO: scrcpy 5.0.1 <https://github.com/Genymobile/scrcpy>"),
            None
        );
    }

    #[test]
    fn exit_codes() {
        assert_eq!(classify_exit(Some(0)), ExitKind::Closed);
        assert_eq!(classify_exit(Some(1)), ExitKind::StartFailed);
        assert_eq!(classify_exit(Some(2)), ExitKind::Disconnected);
        assert_eq!(classify_exit(None), ExitKind::Crashed);
    }
}
