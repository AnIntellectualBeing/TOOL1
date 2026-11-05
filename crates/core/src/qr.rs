//! The QR code Android's "Pair device with QR code" screen scans.
//!
//! Android expects a Wi-Fi style payload: `WIFI:T:ADB;S:<name>;P:<password>;;`.
//! After scanning, the phone advertises `_adb-tls-pairing._tcp` with `<name>` as
//! the mDNS instance name, and accepts `adb pair` with `<password>`.

use qrcode::render::svg;
use qrcode::{EcLevel, QrCode};
use rand::RngExt;

/// Characters used for generated names and passwords. Only letters and digits,
/// so nothing needs escaping and nothing is ambiguous if typed by hand.
const ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingCredentials {
    /// mDNS instance name the phone will advertise, e.g. `phonemirror-Xy7Kq2`.
    pub service_name: String,
    pub password: String,
}

fn random_string(len: usize) -> String {
    let mut rng = rand::rng();
    (0..len)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect()
}

impl PairingCredentials {
    /// Fresh random credentials. A new pair is made for every QR shown, so an
    /// old screenshot of the code can't be used to pair later.
    pub fn generate() -> Self {
        Self {
            service_name: format!("phonemirror-{}", random_string(6)),
            password: random_string(12),
        }
    }

    pub fn qr_payload(&self) -> String {
        format!(
            "WIFI:T:ADB;S:{};P:{};;",
            escape_wifi_field(&self.service_name),
            escape_wifi_field(&self.password)
        )
    }

    /// The QR as a standalone SVG document (dark modules on white, with the
    /// quiet zone that phone cameras need).
    pub fn qr_svg(&self) -> String {
        let code = QrCode::with_error_correction_level(self.qr_payload().as_bytes(), EcLevel::M)
            .expect("payload is short enough for a QR code");
        let svg = code
            .render::<svg::Color<'_>>()
            .min_dimensions(240, 240)
            .dark_color(svg::Color("#000000"))
            .light_color(svg::Color("#ffffff"))
            .quiet_zone(true)
            .build();
        // Drop the XML declaration so it can go straight into HTML.
        match svg.find("<svg") {
            Some(i) => svg[i..].to_string(),
            None => svg,
        }
    }
}

/// Escape `\ ; , : "` as the Wi-Fi QR format requires.
pub fn escape_wifi_field(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if matches!(c, '\\' | ';' | ',' | ':' | '"') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_has_android_format() {
        let c = PairingCredentials {
            service_name: "phonemirror-abc".into(),
            password: "s3cret".into(),
        };
        assert_eq!(c.qr_payload(), "WIFI:T:ADB;S:phonemirror-abc;P:s3cret;;");
    }

    #[test]
    fn generated_credentials_are_random_and_safe() {
        let a = PairingCredentials::generate();
        let b = PairingCredentials::generate();
        assert_ne!(a, b);
        assert!(a.service_name.starts_with("phonemirror-"));
        assert_eq!(a.password.len(), 12);
        for ch in a.password.chars().chain(a.service_name.chars()) {
            assert!(
                ch.is_ascii_alphanumeric() || ch == '-',
                "unexpected char {ch}"
            );
        }
        // Nothing needed escaping.
        assert_eq!(escape_wifi_field(&a.password), a.password);
    }

    #[test]
    fn escapes_special_characters() {
        assert_eq!(escape_wifi_field(r#"a;b,c:d\e"f"#), r#"a\;b\,c\:d\\e\"f"#);
    }

    #[test]
    fn renders_svg() {
        let svg = PairingCredentials::generate().qr_svg();
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("#000000"));
    }
}
