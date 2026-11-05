//! User settings, saved as JSON. Every field has a default so older or
//! hand-edited files keep loading.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    /// Smooth over Wi-Fi, Balanced over USB.
    #[default]
    Auto,
    /// Lowest latency: smaller picture, 60 fps.
    Smooth,
    Balanced,
    /// Full resolution, high bit rate. Best over USB.
    Sharp,
    /// 30 fps and low bit rate, easy on the phone's battery.
    BatterySaver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VideoOutput {
    /// Direct3D 11 on Windows (hardware decoding, Alt+Enter fullscreen).
    #[default]
    Auto,
    /// Direct3D 12 (newer GPUs).
    D3d12,
    /// Software decoding and the generic sink. Slower, but works on odd GPUs/VMs.
    Compatible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// AirPlay name; `None` means "<PC name> – PhoneMirror".
    pub receiver_name: Option<String>,
    pub start_iphone_receiver: bool,
    pub iphone_quality: Quality,
    pub iphone_rotation: u16,
    pub iphone_video_output: VideoOutput,
    /// Allow "AirPlay video" casting from apps like YouTube (UxPlay's HLS mode).
    pub iphone_video_casting: bool,
    /// Ask for a 4-digit code on the PC screen before an iPhone may connect.
    pub iphone_require_pin: bool,

    pub android_quality: Quality,
    pub android_audio: bool,
    pub android_screen_off: bool,
    pub android_stay_awake: bool,
    pub android_clipboard_sync: bool,
    pub android_rotation: u16,

    pub always_on_top: bool,
    pub auto_reconnect: bool,
    pub screenshots_dir: Option<String>,
    pub recordings_dir: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            receiver_name: None,
            start_iphone_receiver: true,
            iphone_quality: Quality::Auto,
            iphone_rotation: 0,
            iphone_video_output: VideoOutput::Auto,
            iphone_video_casting: false,
            iphone_require_pin: false,
            android_quality: Quality::Auto,
            android_audio: true,
            android_screen_off: false,
            android_stay_awake: true,
            android_clipboard_sync: true,
            android_rotation: 0,
            always_on_top: false,
            auto_reconnect: true,
            screenshots_dir: None,
            recordings_dir: None,
        }
    }
}

/// Clamp a rotation to 0/90/180/270.
pub fn normalize_rotation(deg: i32) -> u16 {
    (deg.rem_euclid(360) / 90 * 90) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_json_uses_defaults() {
        let s: Settings = serde_json::from_str(r#"{"android_audio": false}"#).unwrap();
        assert!(!s.android_audio);
        assert!(s.start_iphone_receiver);
        assert_eq!(s.android_quality, Quality::Auto);
    }

    #[test]
    fn rotation_normalizes() {
        assert_eq!(normalize_rotation(450), 90);
        assert_eq!(normalize_rotation(-90), 270);
        assert_eq!(normalize_rotation(100), 90);
    }
}
