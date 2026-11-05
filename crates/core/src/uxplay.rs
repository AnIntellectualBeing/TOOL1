//! UxPlay (AirPlay receiver) command lines and log interpretation.

use crate::errors::Problem;
use crate::settings::{Quality, VideoOutput};
use std::collections::BTreeSet;

/// Default first port. UxPlay uses TCP and UDP `n`, `n+1`, `n+2`.
pub const DEFAULT_PORT: u16 = 7100;

/// The longest receiver name we hand to UxPlay. A DNS-SD instance label is at
/// most 63 bytes, and the AirPlay audio service prefixes `<12 hex>@`.
pub const MAX_NAME_BYTES: usize = 50;

const SUFFIX: &str = " – PhoneMirror";

/// "<PC name> – PhoneMirror", cleaned and cut to fit mDNS limits.
pub fn receiver_name(pc_name: &str, custom: Option<&str>) -> String {
    if let Some(c) = custom.map(str::trim).filter(|c| !c.is_empty()) {
        return truncate_bytes(&clean(c), MAX_NAME_BYTES);
    }
    let pc = clean(pc_name);
    let pc = if pc.is_empty() {
        "This PC".to_string()
    } else {
        pc
    };
    let room = MAX_NAME_BYTES - SUFFIX.len();
    format!("{}{SUFFIX}", truncate_bytes(&pc, room))
}

/// Drop control characters and `@` (UxPlay uses `@` as a separator), collapse
/// whitespace.
fn clean(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() && *c != '@' && *c != '"')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn truncate_bytes(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].trim_end().to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UxPlayOptions {
    pub name: String,
    pub quality: Quality,
    pub rotation: u16,
    pub video_output: VideoOutput,
    /// "Use my PC as a speaker" only: no video window.
    pub audio_only: bool,
    pub audio: bool,
    pub fullscreen: bool,
    /// Base path; UxPlay writes `<base>.<n>.<format>.mp4`.
    pub record_base: Option<String>,
    pub video_casting: bool,
    pub require_pin: bool,
    pub key_file: Option<String>,
    pub port: Option<u16>,
    pub windows: bool,
}

impl UxPlayOptions {
    pub fn new(name: impl Into<String>, windows: bool) -> Self {
        Self {
            name: name.into(),
            quality: Quality::Auto,
            rotation: 0,
            video_output: VideoOutput::Auto,
            audio_only: false,
            audio: true,
            fullscreen: false,
            record_base: None,
            video_casting: false,
            require_pin: false,
            key_file: None,
            port: Some(DEFAULT_PORT),
            windows,
        }
    }

    pub fn args(&self) -> Vec<String> {
        let mut a: Vec<String> = vec!["-n".into(), self.name.clone(), "-nh".into()];
        let (res, fps) = match self.quality {
            Quality::Auto | Quality::Balanced => ("1920x1080@60", 60),
            Quality::Smooth => ("1280x720@60", 60),
            Quality::Sharp => ("2560x1440@60", 60),
            Quality::BatterySaver => ("1280x720@30", 30),
        };
        a.extend(["-s".into(), res.into(), "-fps".into(), fps.to_string()]);
        if let Some(p) = self.port {
            a.extend(["-p".into(), p.to_string()]);
        }
        if let Some(k) = &self.key_file {
            // A stable key means the iPhone sees the same receiver every time.
            a.extend(["-key".into(), k.clone()]);
        }
        if self.audio_only {
            a.extend(["-vs".into(), "0".into()]);
        } else if let Some(sink) = self.video_sink() {
            a.extend(["-vs".into(), sink.into()]);
        }
        if self.video_output == VideoOutput::Compatible && !self.audio_only {
            a.push("-avdec".into());
        }
        if !self.audio {
            a.extend(["-as".into(), "0".into()]);
        } else if self.windows {
            a.extend(["-as".into(), "wasapisink".into()]);
        }
        match self.rotation {
            90 => a.extend(["-r".into(), "R".into()]),
            180 => a.extend(["-f".into(), "I".into()]),
            270 => a.extend(["-r".into(), "L".into()]),
            _ => {}
        }
        if self.fullscreen && !self.audio_only {
            a.push("-fs".into());
        }
        if let Some(base) = &self.record_base {
            a.extend(["-mp4".into(), base.clone()]);
        }
        if self.video_casting {
            a.push("-hls".into());
        }
        if self.require_pin {
            a.push("-pin".into());
        }
        a
    }

    fn video_sink(&self) -> Option<&'static str> {
        if !self.windows {
            return None; // UxPlay's autovideosink choice is right on Linux/macOS.
        }
        Some(match self.video_output {
            VideoOutput::Auto => "d3d11videosink",
            VideoOutput::D3d12 => "d3d12videosink",
            VideoOutput::Compatible => "autovideosink",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UxPlayEvent {
    /// An iPhone/iPad/Mac started a session.
    ClientConnected {
        name: String,
        model: String,
    },
    /// What the client is sending: mirrored screen, or audio only (music).
    Mode {
        mirroring: bool,
    },
    ClientDisconnected,
    /// Show this code so the person can type it on the iPhone.
    Pin(String),
    /// Video casting (YouTube) started.
    CastingStarted,
    Problem {
        problem: Problem,
        fatal: bool,
    },
}

/// Follows UxPlay's log to know whether an iPhone is connected.
#[derive(Debug, Default)]
pub struct UxPlayMonitor {
    sockets: BTreeSet<u32>,
    connected: bool,
}

fn number_after(line: &str, key: &str) -> Option<u32> {
    let rest = line.split(key).nth(1)?;
    rest.trim_start()
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

impl UxPlayMonitor {
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn feed(&mut self, line: &str) -> Vec<UxPlayEvent> {
        let l = line.trim();
        let mut ev = Vec::new();
        let problem = |p: Problem, fatal: bool| UxPlayEvent::Problem {
            problem: p.details(l),
            fatal,
        };

        if l.contains("Accepted ") && l.contains(" client on socket") {
            if let Some(n) = number_after(l, "on socket") {
                self.sockets.insert(n);
            }
        } else if l.contains("Connection closed on socket")
            || l.contains("Removing connection for socket")
        {
            let key = if l.contains("closed on socket") {
                "closed on socket"
            } else {
                "connection for socket"
            };
            if let Some(n) = number_after(l, key) {
                self.sockets.remove(&n);
            }
            if self.sockets.is_empty() && self.connected {
                self.connected = false;
                ev.push(UxPlayEvent::ClientDisconnected);
            }
        } else if let Some(rest) = l.split("connection request from ").nth(1) {
            // "<name> (<model>) with deviceID = <id>"
            let before_id = rest.split(" with deviceID").next().unwrap_or(rest);
            let (name, model) = match before_id.rfind(" (") {
                Some(i) => (&before_id[..i], before_id[i + 2..].trim_end_matches(')')),
                None => (before_id, ""),
            };
            self.connected = true;
            ev.push(UxPlayEvent::ClientConnected {
                name: name.trim().into(),
                model: model.trim().into(),
            });
        } else if l.contains("usingScreen=") {
            let mirroring = number_after(l, "usingScreen=").unwrap_or(1) == 1;
            ev.push(UxPlayEvent::Mode { mirroring });
        } else if l.contains("lost connection with client") {
            self.connected = false;
            self.sockets.clear();
            ev.push(UxPlayEvent::ClientDisconnected);
        } else if let Some(rest) = l.split("ENTER PIN = \"").nth(1) {
            ev.push(UxPlayEvent::Pin(
                rest.split('"').next().unwrap_or("").to_string(),
            ));
        } else if l.contains("on_video_play") {
            ev.push(UxPlayEvent::CastingStarted);
        } else if l.contains("dnssd_register_raop failed")
            || l.contains("dnssd_register_airplay failed")
            || l.contains("Could not initialize dnssd")
        {
            ev.push(problem(Problem::receiver_cannot_advertise(), true));
        } else if l.contains("Error initializing raop") || l.contains("Error initializing HTTP") {
            ev.push(problem(
                Problem::new("receiver_start_failed", "The iPhone receiver couldn't start.")
                    .hint("Another app may be using its network ports (7100–7102). Close other AirPlay or mirroring apps and press \"Restart receiver\"."),
                true,
            ));
        } else if l.contains("GStreamer error (video)") {
            ev.push(problem(Problem::video_decoder_failed(l), false));
        } else if l.contains("GStreamer error (audio)") {
            ev.push(problem(
                Problem::new("audio_failed", "Sound couldn't be played on this PC.")
                    .hint("Check that a speaker or headphones are connected."),
                false,
            ));
        } else if l.contains("no element \"") || l.contains("No such element") {
            ev.push(problem(
                Problem::new("engine_incomplete", "Part of the iPhone engine is missing.")
                    .hint("Reinstall PhoneMirror."),
                true,
            ));
        }
        ev
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friendly_name() {
        assert_eq!(
            receiver_name("DESKTOP-4FQ2", None),
            "DESKTOP-4FQ2 – PhoneMirror"
        );
        assert_eq!(receiver_name("  ", None), "This PC – PhoneMirror");
        assert_eq!(receiver_name("x", Some(" Living room ")), "Living room");
        assert_eq!(receiver_name("bad@name\n", None), "badname – PhoneMirror");
    }

    #[test]
    fn long_names_fit_mdns() {
        let long = "Ünïcödé-".repeat(20);
        let n = receiver_name(&long, None);
        assert!(n.len() <= MAX_NAME_BYTES, "{} bytes", n.len());
        assert!(n.ends_with(SUFFIX));
        let custom = receiver_name("x", Some(&"é".repeat(60)));
        assert!(custom.len() <= MAX_NAME_BYTES);
    }

    #[test]
    fn default_windows_args() {
        let mut o = UxPlayOptions::new("PC – PhoneMirror", true);
        o.key_file = Some("C:\\data\\uxplay.pem".into());
        let a = o.args();
        let joined = a.join(" ");
        assert!(joined.starts_with("-n PC – PhoneMirror -nh"));
        assert!(joined.contains("-s 1920x1080@60 -fps 60"));
        assert!(joined.contains("-p 7100"));
        assert!(joined.contains("-key C:\\data\\uxplay.pem"));
        assert!(joined.contains("-vs d3d11videosink"));
        assert!(joined.contains("-as wasapisink"));
        assert!(!joined.contains("-hls"));
        assert!(!joined.contains("-avdec"));
    }

    #[test]
    fn options_map_to_flags() {
        let mut o = UxPlayOptions::new("N", true);
        o.rotation = 270;
        o.fullscreen = true;
        o.record_base = Some("C:\\Videos\\iPhone".into());
        o.video_casting = true;
        o.require_pin = true;
        o.video_output = VideoOutput::Compatible;
        o.quality = Quality::BatterySaver;
        let j = o.args().join(" ");
        for want in [
            "-r L",
            "-fs",
            "-mp4 C:\\Videos\\iPhone",
            "-hls",
            "-pin",
            "-avdec",
            "-vs autovideosink",
            "-fps 30",
        ] {
            assert!(j.contains(want), "missing {want} in {j}");
        }
    }

    #[test]
    fn speaker_mode_has_no_video() {
        let mut o = UxPlayOptions::new("N", true);
        o.audio_only = true;
        o.fullscreen = true;
        let j = o.args().join(" ");
        assert!(j.contains("-vs 0"));
        assert!(!j.contains("-fs"));
        assert!(!j.contains("d3d11"));
    }

    #[test]
    fn linux_uses_uxplay_defaults() {
        let j = UxPlayOptions::new("N", false).args().join(" ");
        assert!(!j.contains("-vs"));
        assert!(!j.contains("-as"));
    }

    #[test]
    fn follows_a_session() {
        let mut m = UxPlayMonitor::default();
        assert!(m
            .feed("Accepted IPv4 client on socket 23, port 7100")
            .is_empty());
        assert!(m
            .feed("Accepted IPv4 client on socket 25, port 7100")
            .is_empty());
        let ev = m.feed(
            "connection request from Jane’s iPhone (iPhone15,2) with deviceID = AA:BB:CC:DD:EE:FF",
        );
        assert_eq!(
            ev,
            [UxPlayEvent::ClientConnected {
                name: "Jane’s iPhone".into(),
                model: "iPhone15,2".into()
            }]
        );
        assert!(m.is_connected());
        assert_eq!(
            m.feed("ct=8 spf=480 usingScreen=1 isMedia=0  audioFormat=0x1000000"),
            [UxPlayEvent::Mode { mirroring: true }]
        );
        assert!(m.feed("Connection closed on socket 23").is_empty());
        assert_eq!(
            m.feed("Connection closed on socket 25"),
            [UxPlayEvent::ClientDisconnected]
        );
        assert!(!m.is_connected());
    }

    #[test]
    fn audio_only_session() {
        let mut m = UxPlayMonitor::default();
        m.feed("Accepted IPv6 client on socket 7, port 7100");
        m.feed("connection request from iPad (iPad13,4) with deviceID = X");
        assert_eq!(
            m.feed("ct=2 spf=352 usingScreen=0 isMedia=1  audioFormat=0x40000"),
            [UxPlayEvent::Mode { mirroring: false }]
        );
    }

    #[test]
    fn lost_connection_and_pin() {
        let mut m = UxPlayMonitor::default();
        m.feed("Accepted IPv4 client on socket 3, port 7100");
        m.feed("connection request from A (iPhone14,5) with deviceID = X");
        assert_eq!(
            m.feed("*** ERROR lost connection with client (network problem?)"),
            [UxPlayEvent::ClientDisconnected]
        );
        assert_eq!(
            m.feed("*** CLIENT MUST NOW ENTER PIN = \"4821\" AS AIRPLAY PASSWORD"),
            [UxPlayEvent::Pin("4821".into())]
        );
    }

    #[test]
    fn problems() {
        let mut m = UxPlayMonitor::default();
        let ev = m.feed("dnssd_register_raop failed with error code -65537");
        assert!(
            matches!(&ev[0], UxPlayEvent::Problem { problem, fatal: true } if problem.code == "advertise_failed")
        );
        let ev = m.feed("GStreamer error (video): d3d11videosink0 Failed to create device");
        assert!(
            matches!(&ev[0], UxPlayEvent::Problem { problem, fatal: false } if problem.code == "video_failed")
        );
        let ev = m.feed("WARNING: erroneous pipeline: no element \"avdec_aac\"");
        assert!(
            matches!(&ev[0], UxPlayEvent::Problem { problem, .. } if problem.code == "engine_incomplete")
        );
    }
}
