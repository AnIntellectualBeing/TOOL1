//! Print a pairing QR as SVG (used for the UI demo mode and for testing a
//! phone's scanner by hand): `cargo run -p phonemirror-core --example qr_svg`.
use phonemirror_core::qr::PairingCredentials;

fn main() {
    let creds = PairingCredentials::generate();
    eprintln!("payload: {}", creds.qr_payload());
    println!("{}", creds.qr_svg());
}
