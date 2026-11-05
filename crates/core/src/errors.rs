//! Plain-language problems shown to people.
//!
//! Every failure that reaches the user interface goes through [`Problem`]. A
//! problem has a stable `code` (so the UI can pick an icon or a fix button), a
//! short `message` written for someone who has never heard of adb, and an
//! optional `hint` that says what to do next.

use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Raw engine output, kept for the "Details" disclosure and bug reports.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(hint) = &self.hint {
            write!(f, " {hint}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Problem {}

impl Problem {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: None,
            details: None,
        }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn details(mut self, details: impl Into<String>) -> Self {
        let d = details.into();
        if !d.trim().is_empty() {
            self.details = Some(d.trim().to_string());
        }
        self
    }

    // ----- Network -----

    pub fn no_network() -> Self {
        Self::new(
            "no_network",
            "Your PC isn't connected to a Wi-Fi or wired network.",
        )
        .hint("Connect the PC to the same Wi-Fi as your phone, then try again.")
    }

    pub fn different_network(phone_ip: &str) -> Self {
        Self::new(
            "different_network",
            "Your phone and PC are on different Wi-Fi networks.",
        )
        .hint(format!(
            "Your phone has the address {phone_ip}, which isn't on the PC's network. \
                 Connect both to the same Wi-Fi (not a guest network), or turn on your \
                 phone's hotspot and connect the PC to it."
        ))
    }

    pub fn network_blocks_devices() -> Self {
        Self::new(
            "network_blocks_devices",
            "This Wi-Fi seems to stop devices from seeing each other.",
        )
        .hint(
            "Public, hotel, school and office Wi-Fi often do this. Turn on your phone's \
             hotspot, connect the PC to it, and try again.",
        )
    }

    // ----- Android -----

    pub fn adb_missing(path: &str) -> Self {
        Self::new("engine_missing", "PhoneMirror's Android engine is missing.")
            .hint(
                "Reinstall PhoneMirror. If you built it yourself, run scripts/fetch-engines first.",
            )
            .details(format!("adb not found at {path}"))
    }

    pub fn no_android_device() -> Self {
        Self::new("no_device", "No Android phone found.").hint(
            "Plug the phone in with a USB cable that carries data (some cables only \
                 charge), or connect over Wi-Fi with \"Add a phone\".",
        )
    }

    pub fn unauthorized() -> Self {
        Self::new("unauthorized", "Your phone is asking for permission.").hint(
            "Unlock the phone and tap \"Allow\" on the \"Allow USB debugging?\" message. \
                 Tick \"Always allow from this computer\" so it doesn't ask again.",
        )
    }

    pub fn device_offline() -> Self {
        Self::new("offline", "Your phone stopped answering.")
            .hint("Unplug the cable and plug it in again, or turn Wireless debugging off and on.")
    }

    pub fn no_usb_permission() -> Self {
        Self::new("no_permissions", "Your computer won't let PhoneMirror use this USB device.")
            .hint("On Linux, add a udev rule for Android devices. See the README's troubleshooting section.")
    }

    pub fn pairing_not_seen(waited_secs: u64) -> Self {
        Self::new(
            "pairing_timeout",
            format!("We didn't see your phone scan the code ({waited_secs} seconds)."),
        )
        .hint(
            "On the phone open Settings → Developer options → Wireless debugging → \"Pair \
             device with QR code\" and scan the code again. Phone and PC must be on the same Wi-Fi.",
        )
    }

    pub fn pairing_wrong_code() -> Self {
        Self::new("pairing_failed", "Pairing didn't work.").hint(
            "The code may have expired or been typed wrong. Close the pairing screen on \
                 the phone and try again with a fresh code.",
        )
    }

    pub fn phone_not_visible(name: &str) -> Self {
        Self::new(
            "phone_not_visible",
            format!("We can't find {name} on this Wi-Fi."),
        )
        .hint(
            "On the phone, check that Wireless debugging is on (Settings → Developer options \
                 → Wireless debugging). It turns itself off after a restart or when the phone \
                 changes Wi-Fi. Phone and PC must be on the same Wi-Fi.",
        )
    }

    pub fn not_paired_anymore(name: &str) -> Self {
        Self::new("not_paired", format!("{name} doesn't recognise this PC anymore."))
            .hint("This happens after \"Revoke USB debugging authorisations\" or a reset. Pair it again with \"Add a phone\".")
    }

    pub fn connection_refused() -> Self {
        Self::new("connection_refused", "The phone refused the connection.")
            .hint("Wireless debugging may have been switched off or restarted on the phone. Turn it on again and retry.")
    }

    pub fn android_too_old_for_wireless(version: &str) -> Self {
        Self::new(
            "android_too_old",
            format!(
                "Wireless pairing needs Android 11 or newer (this phone has Android {version})."
            ),
        )
        .hint("Use a USB cable instead. Everything else works the same.")
    }

    pub fn audio_needs_android_11() -> Self {
        Self::new("audio_unsupported", "Sound needs Android 11 or newer.")
            .hint("Mirroring continues without sound.")
    }

    pub fn audio_needs_unlock() -> Self {
        Self::new(
            "audio_needs_unlock",
            "Your phone was locked, so sound couldn't start.",
        )
        .hint("Unlock the phone and press \"Restart\" to get sound (Android 11 asks for this).")
    }

    pub fn input_blocked_by_brand() -> Self {
        Self::new(
            "input_blocked",
            "Your phone blocks mouse and keyboard control.",
        )
        .hint(
            "Xiaomi/Redmi/POCO: in Developer options turn on \"USB debugging (Security \
                 settings)\" and restart the phone. OPPO/Realme: turn on \"Disable permission \
                 monitoring\". Viewing works without it.",
        )
    }

    pub fn camera_needs_android_12() -> Self {
        Self::new(
            "camera_unsupported",
            "Using the phone camera needs Android 12 or newer.",
        )
    }

    // ----- iPhone -----

    pub fn receiver_crashed(details: &str) -> Self {
        Self::new("receiver_crashed", "The iPhone receiver stopped unexpectedly.")
            .hint("PhoneMirror restarted it. If this keeps happening, use \"Copy diagnostics\" and report it.")
            .details(details)
    }

    pub fn receiver_cannot_advertise() -> Self {
        Self::new(
            "advertise_failed",
            "The PC couldn't announce itself to iPhones on this network.",
        )
        .hint(
            "Another AirPlay receiver may be running, or the firewall is blocking it. Close \
             other mirroring apps and press \"Fix firewall\".",
        )
    }

    pub fn firewall_blocking() -> Self {
        Self::new(
            "firewall",
            "Windows Firewall may be blocking iPhones from finding this PC.",
        )
        .hint(
            "Press \"Fix firewall\". Windows will ask for permission once. This only allows \
                 PhoneMirror to receive the picture and sound from devices on your local network.",
        )
    }

    pub fn public_network() -> Self {
        Self::new(
            "public_network",
            "Windows treats this Wi-Fi as a public network.",
        )
        .hint(
            "On public networks Windows hides the PC from other devices. If this is your \
                 home Wi-Fi, set it to Private (Settings → Network & internet → Wi-Fi → your \
                 network → Private), or press \"Allow on this network\".",
        )
    }

    pub fn video_decoder_failed(details: &str) -> Self {
        Self::new("video_failed", "The picture couldn't be shown.")
            .hint(
                "Try the \"Compatible\" video option in Settings, then reconnect from the iPhone.",
            )
            .details(details)
    }

    pub fn engine_missing(which: &str, path: &str) -> Self {
        Self::new(
            "engine_missing",
            format!("PhoneMirror's {which} engine is missing."),
        )
        .hint("Reinstall PhoneMirror. If you built it yourself, see BUILDING.md.")
        .details(format!("not found: {path}"))
    }

    pub fn internal(details: impl Into<String>) -> Self {
        Self::new("internal", "Something went wrong inside PhoneMirror.")
            .hint("Try again. If it keeps happening, use \"Copy diagnostics\" and report it.")
            .details(details)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_includes_hint() {
        let p = Problem::new("x", "Main.").hint("Do this.");
        assert_eq!(p.to_string(), "Main. Do this.");
    }

    #[test]
    fn empty_details_are_dropped() {
        let p = Problem::new("x", "m").details("   \n");
        assert!(p.details.is_none());
    }

    #[test]
    fn serializes_without_empty_fields() {
        let json = serde_json::to_string(&Problem::new("x", "m")).unwrap();
        assert_eq!(json, r#"{"code":"x","message":"m"}"#);
    }

    #[test]
    fn different_network_mentions_ip() {
        let p = Problem::different_network("10.0.0.7");
        assert!(p.hint.unwrap().contains("10.0.0.7"));
        assert_eq!(
            p.message,
            "Your phone and PC are on different Wi-Fi networks."
        );
    }
}
