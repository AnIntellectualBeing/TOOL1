//! Talking to adb, through a [`CommandRunner`] so tests can replace the real
//! binary with a script of canned answers.

use crate::errors::Problem;
use async_trait::async_trait;
use serde::Serialize;
use std::time::Duration;

/// Result of running an external program.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmdOutput {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl CmdOutput {
    pub fn ok(stdout: impl Into<String>) -> Self {
        Self {
            code: Some(0),
            stdout: stdout.into().into_bytes(),
            stderr: vec![],
        }
    }
    pub fn fail(code: i32, stderr: impl Into<String>) -> Self {
        Self {
            code: Some(code),
            stdout: vec![],
            stderr: stderr.into().into_bytes(),
        }
    }
    pub fn stdout_str(&self) -> String {
        String::from_utf8_lossy(&self.stdout).replace("\r\n", "\n")
    }
    pub fn stderr_str(&self) -> String {
        String::from_utf8_lossy(&self.stderr).replace("\r\n", "\n")
    }
    /// stdout and stderr together; adb is inconsistent about which it uses.
    pub fn text(&self) -> String {
        let mut s = self.stdout_str();
        let e = self.stderr_str();
        if !e.is_empty() {
            if !s.is_empty() && !s.ends_with('\n') {
                s.push('\n');
            }
            s.push_str(&e);
        }
        s
    }
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    /// The program doesn't exist at the configured path.
    NotFound(String),
    TimedOut,
    Other(String),
}

/// Runs adb with the given arguments.
#[async_trait]
pub trait CommandRunner: Send + Sync {
    async fn run(&self, args: &[String], timeout: Duration) -> Result<CmdOutput, RunError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    Device,
    Unauthorized,
    Offline,
    NoPermissions,
    Authorizing,
    Connecting,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    Usb,
    Wifi,
    Emulator,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdbDevice {
    pub serial: String,
    pub state: DeviceState,
    pub transport: Transport,
    pub model: Option<String>,
    pub product: Option<String>,
    pub device: Option<String>,
}

impl AdbDevice {
    /// "Pixel 7" from "Pixel_7".
    pub fn display_model(&self) -> Option<String> {
        self.model.as_ref().map(|m| m.replace('_', " "))
    }

    /// For mDNS-auto-connected devices adb names them
    /// `adb-<serial>-<suffix>._adb-tls-connect._tcp`; return `adb-<serial>-<suffix>`.
    pub fn mdns_instance(&self) -> Option<&str> {
        self.serial.strip_suffix("._adb-tls-connect._tcp")
    }
}

pub fn classify_transport(serial: &str) -> Transport {
    if serial.starts_with("emulator-") {
        Transport::Emulator
    } else if serial.contains("._adb-tls-connect._tcp")
        || crate::net::parse_host_port(serial).is_some()
    {
        Transport::Wifi
    } else {
        Transport::Usb
    }
}

/// Parse the output of `adb devices -l`.
pub fn parse_devices(output: &str) -> Vec<AdbDevice> {
    let mut out = Vec::new();
    for raw in output.lines() {
        let line = raw.trim();
        if line.is_empty()
            || line.starts_with("List of devices")
            || line.starts_with('*')
            || line.starts_with("adb server")
            || line.starts_with("adb: ")
        {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(serial) = parts.next() else { continue };
        let rest: Vec<&str> = parts.collect();
        let rest_joined = rest.join(" ");
        let state = if rest_joined.starts_with("no permissions") {
            DeviceState::NoPermissions
        } else {
            match rest.first().copied() {
                Some("device") => DeviceState::Device,
                Some("unauthorized") => DeviceState::Unauthorized,
                Some("offline") => DeviceState::Offline,
                Some("authorizing") => DeviceState::Authorizing,
                Some("connecting") => DeviceState::Connecting,
                _ => DeviceState::Other,
            }
        };
        let field = |key: &str| {
            rest.iter()
                .find_map(|kv| kv.strip_prefix(key).and_then(|v| v.strip_prefix(':')))
                .map(str::to_string)
        };
        out.push(AdbDevice {
            serial: serial.to_string(),
            state,
            transport: classify_transport(serial),
            model: field("model"),
            product: field("product"),
            device: field("device"),
        });
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairOutcome {
    /// mDNS instance name of the phone's `_adb-tls-connect` service, if adb
    /// reported it (adb >= 30 does: `[guid=adb-XXXX-yyyy]`).
    pub guid: Option<String>,
}

/// Interpret `adb pair` output.
pub fn parse_pair(out: &CmdOutput) -> Result<PairOutcome, Problem> {
    let text = out.text();
    if let Some(line) = text.lines().find(|l| l.contains("Successfully paired")) {
        let guid = line
            .split("[guid=")
            .nth(1)
            .and_then(|r| r.split(']').next())
            .map(str::to_string)
            .filter(|g| !g.is_empty());
        return Ok(PairOutcome { guid });
    }
    let lower = text.to_lowercase();
    if lower.contains("wrong password")
        || lower.contains("protocol fault")
        || lower.contains("unable to start pairing")
    {
        return Err(Problem::pairing_wrong_code().details(text));
    }
    if lower.contains("refused") || lower.contains("10061") {
        return Err(Problem::connection_refused().details(text));
    }
    Err(Problem::pairing_wrong_code().details(text))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectOutcome {
    Connected,
    AlreadyConnected,
}

/// Interpret `adb connect` output. adb exits 0 even on failure, so read the text.
pub fn parse_connect(out: &CmdOutput, display_name: &str) -> Result<ConnectOutcome, Problem> {
    let text = out.text();
    let lower = text.to_lowercase();
    if lower.contains("already connected to") {
        return Ok(ConnectOutcome::AlreadyConnected);
    }
    if lower.contains("failed to authenticate") {
        return Err(Problem::not_paired_anymore(display_name).details(text));
    }
    if lower.contains("refused") || lower.contains("10061") {
        return Err(Problem::connection_refused().details(text));
    }
    if lower.contains("timed out")
        || lower.contains("10060")
        || lower.contains("no route to host")
        || lower.contains("network is unreachable")
        || lower.contains("10065")
        || lower.contains("10051")
    {
        return Err(Problem::phone_not_visible(display_name).details(text));
    }
    if lower.contains("connected to") && !lower.contains("failed") && !lower.contains("cannot") {
        return Ok(ConnectOutcome::Connected);
    }
    Err(Problem::phone_not_visible(display_name).details(text))
}

/// Facts read from the phone with `getprop`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DeviceInfo {
    pub manufacturer: String,
    pub model: String,
    pub android_version: String,
    pub sdk: u32,
}

impl DeviceInfo {
    /// "Samsung Galaxy S21" style name; avoids "Google Google Pixel 7".
    pub fn friendly_name(&self) -> String {
        let manu = title_case(&self.manufacturer);
        if self.model.is_empty() {
            return if manu.is_empty() {
                "Android phone".into()
            } else {
                manu
            };
        }
        if manu.is_empty()
            || self
                .model
                .to_lowercase()
                .starts_with(&self.manufacturer.to_lowercase())
        {
            self.model.clone()
        } else {
            format!("{manu} {}", self.model)
        }
    }
}

fn title_case(s: &str) -> String {
    let mut c = s.trim().chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

/// Parse `adb shell getprop` (full listing) lines like `[ro.product.model]: [Pixel 7]`.
pub fn parse_getprop(output: &str) -> DeviceInfo {
    let mut info = DeviceInfo::default();
    for line in output.lines() {
        let Some((k, v)) = line.split_once("]: [") else {
            continue;
        };
        let key = k.trim().trim_start_matches('[');
        let val = v.trim().trim_end_matches(']').to_string();
        match key {
            "ro.product.manufacturer" => info.manufacturer = val,
            "ro.product.model" => info.model = val,
            "ro.build.version.release" => info.android_version = val,
            "ro.build.version.sdk" => info.sdk = val.parse().unwrap_or(0),
            _ => {}
        }
    }
    info
}

/// High-level adb operations.
pub struct Adb<'a> {
    runner: &'a dyn CommandRunner,
}

const SHORT: Duration = Duration::from_secs(10);
const LONG: Duration = Duration::from_secs(25);

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn map_run_error(e: RunError) -> Problem {
    match e {
        RunError::NotFound(path) => Problem::adb_missing(&path),
        RunError::TimedOut => Problem::new("timeout", "The phone took too long to answer.")
            .hint("Check the cable or Wi-Fi and try again."),
        RunError::Other(msg) => Problem::internal(msg),
    }
}

impl<'a> Adb<'a> {
    pub fn new(runner: &'a dyn CommandRunner) -> Self {
        Self { runner }
    }

    async fn run(&self, args: Vec<String>, timeout: Duration) -> Result<CmdOutput, Problem> {
        self.runner.run(&args, timeout).await.map_err(map_run_error)
    }

    pub async fn devices(&self) -> Result<Vec<AdbDevice>, Problem> {
        let out = self.run(s(&["devices", "-l"]), LONG).await?;
        Ok(parse_devices(&out.stdout_str()))
    }

    pub async fn pair(&self, target: &str, password: &str) -> Result<PairOutcome, Problem> {
        let out = self.run(s(&["pair", target, password]), LONG).await?;
        parse_pair(&out)
    }

    pub async fn connect(
        &self,
        target: &str,
        display_name: &str,
    ) -> Result<ConnectOutcome, Problem> {
        let out = self.run(s(&["connect", target]), LONG).await?;
        parse_connect(&out, display_name)
    }

    pub async fn disconnect(&self, target: &str) -> Result<(), Problem> {
        self.run(s(&["disconnect", target]), SHORT)
            .await
            .map(|_| ())
    }

    pub async fn device_info(&self, serial: &str) -> Result<DeviceInfo, Problem> {
        let out = self
            .run(s(&["-s", serial, "shell", "getprop"]), SHORT)
            .await?;
        if !out.success() {
            return Err(classify_shell_failure(&out));
        }
        Ok(parse_getprop(&out.stdout_str()))
    }

    /// Install an APK (replacing an existing version).
    pub async fn install(&self, serial: &str, apk: &str) -> Result<(), Problem> {
        let out = self
            .run(
                s(&["-s", serial, "install", "-r", apk]),
                Duration::from_secs(300),
            )
            .await?;
        let text = out.text();
        if text.contains("Success") {
            return Ok(());
        }
        let reason = text
            .lines()
            .find(|l| l.contains("Failure") || l.contains("INSTALL_FAILED"))
            .unwrap_or("")
            .to_string();
        Err(install_problem(&reason).details(text))
    }

    /// Copy a file to the phone's Download folder.
    pub async fn push(&self, serial: &str, local: &str, remote_dir: &str) -> Result<(), Problem> {
        let out = self
            .run(
                s(&["-s", serial, "push", local, remote_dir]),
                Duration::from_secs(600),
            )
            .await?;
        if out.success() {
            Ok(())
        } else {
            Err(
                Problem::new("push_failed", "The file couldn't be copied to the phone.")
                    .hint("Check there's enough free space on the phone.")
                    .details(out.text()),
            )
        }
    }

    /// Full-resolution screenshot as PNG bytes.
    pub async fn screenshot_png(&self, serial: &str) -> Result<Vec<u8>, Problem> {
        let out = self
            .run(s(&["-s", serial, "exec-out", "screencap", "-p"]), LONG)
            .await?;
        if out.success() && out.stdout.starts_with(b"\x89PNG") {
            Ok(out.stdout)
        } else {
            Err(
                Problem::new("screenshot_failed", "The phone didn't send a screenshot.")
                    .details(out.text()),
            )
        }
    }
}

fn classify_shell_failure(out: &CmdOutput) -> Problem {
    let text = out.text();
    let lower = text.to_lowercase();
    if lower.contains("unauthorized") {
        Problem::unauthorized().details(text)
    } else if lower.contains("offline") {
        Problem::device_offline().details(text)
    } else if lower.contains("not found") || lower.contains("no devices") {
        Problem::no_android_device().details(text)
    } else {
        Problem::internal(text)
    }
}

fn install_problem(reason: &str) -> Problem {
    let r = reason.to_uppercase();
    if r.contains("INSUFFICIENT_STORAGE") {
        Problem::new(
            "install_failed",
            "The phone doesn't have enough space for this app.",
        )
    } else if r.contains("UPDATE_INCOMPATIBLE") || r.contains("SIGNATURES") {
        Problem::new(
            "install_failed",
            "A different version of this app is already installed.",
        )
        .hint("Uninstall the app on the phone first, then drop the APK again.")
    } else if r.contains("OLDER_SDK") || r.contains("DEPRECATED_SDK") {
        Problem::new(
            "install_failed",
            "This app doesn't support the phone's Android version.",
        )
    } else if r.contains("USER_RESTRICTED") || r.contains("VERIFICATION") {
        Problem::new("install_failed", "The phone blocked the install.")
            .hint("Unlock the phone and allow the install. On Xiaomi phones turn on \"Install via USB\" in Developer options.")
    } else if r.contains("NO_MATCHING_ABIS") {
        Problem::new(
            "install_failed",
            "This app isn't made for this phone's processor.",
        )
    } else {
        Problem::new("install_failed", "The app couldn't be installed.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICES: &str = "List of devices attached\n\
R58M12345ABC           device usb:1-1 product:beyond1lteeea model:SM_G973F device:beyond1 transport_id:1\n\
192.168.1.23:37415     device product:panther model:Pixel_7 device:panther transport_id:2\n\
adb-28041FDH2000DP-aBcDeF._adb-tls-connect._tcp device product:panther model:Pixel_7 device:panther transport_id:5\n\
0123456789ABCDEF       unauthorized usb:1-2 transport_id:3\n\
emulator-5554          offline transport_id:4\n\
ZY22ABCD               no permissions (missing udev rules? user is in the plugdev group); see [http://developer.android.com/tools/device.html] usb:3-1\n";

    #[test]
    fn parses_devices_list() {
        let d = parse_devices(DEVICES);
        assert_eq!(d.len(), 6);
        assert_eq!(d[0].serial, "R58M12345ABC");
        assert_eq!(d[0].state, DeviceState::Device);
        assert_eq!(d[0].transport, Transport::Usb);
        assert_eq!(d[0].display_model().as_deref(), Some("SM G973F"));
        assert_eq!(d[1].transport, Transport::Wifi);
        assert_eq!(d[2].transport, Transport::Wifi);
        assert_eq!(d[2].mdns_instance(), Some("adb-28041FDH2000DP-aBcDeF"));
        assert_eq!(d[3].state, DeviceState::Unauthorized);
        assert_eq!(d[4].transport, Transport::Emulator);
        assert_eq!(d[4].state, DeviceState::Offline);
        assert_eq!(d[5].state, DeviceState::NoPermissions);
    }

    #[test]
    fn ignores_daemon_banner_and_crlf() {
        let out = "* daemon not running; starting now at tcp:5037\r\n* daemon started successfully\r\nList of devices attached\r\n\r\n";
        assert!(parse_devices(&out.replace("\r\n", "\n")).is_empty());
    }

    #[test]
    fn pair_success_with_guid() {
        let out = CmdOutput::ok(
            "Successfully paired to 192.168.1.23:37099 [guid=adb-28041FDH2000DP-aBcDeF]\n",
        );
        assert_eq!(
            parse_pair(&out).unwrap().guid.as_deref(),
            Some("adb-28041FDH2000DP-aBcDeF")
        );
    }

    #[test]
    fn pair_success_without_guid() {
        let out = CmdOutput::ok("Successfully paired to 192.168.1.23:37099\n");
        assert_eq!(parse_pair(&out).unwrap().guid, None);
    }

    #[test]
    fn pair_wrong_password() {
        let out = CmdOutput::fail(1, "Failed: Wrong password or connection was dropped.\n");
        assert_eq!(parse_pair(&out).unwrap_err().code, "pairing_failed");
        let out = CmdOutput::fail(
            1,
            "error: protocol fault (couldn't read status message): Success\n",
        );
        assert_eq!(parse_pair(&out).unwrap_err().code, "pairing_failed");
    }

    #[test]
    fn connect_outcomes() {
        let c = |t: &str| parse_connect(&CmdOutput::ok(t), "Pixel 7");
        assert_eq!(
            c("connected to 192.168.1.23:41235").unwrap(),
            ConnectOutcome::Connected
        );
        assert_eq!(
            c("already connected to 192.168.1.23:41235").unwrap(),
            ConnectOutcome::AlreadyConnected
        );
        assert_eq!(
            c("failed to authenticate to 192.168.1.23:41235")
                .unwrap_err()
                .code,
            "not_paired"
        );
        assert_eq!(
            c("failed to connect to '192.168.1.23:41235': Connection refused")
                .unwrap_err()
                .code,
            "connection_refused"
        );
        assert_eq!(
            c("cannot connect to 192.168.1.23:5555: No connection could be made because the target machine actively refused it. (10061)")
                .unwrap_err()
                .code,
            "connection_refused"
        );
        assert_eq!(
            c("failed to connect to '10.0.0.2:5555': Connection timed out")
                .unwrap_err()
                .code,
            "phone_not_visible"
        );
    }

    #[test]
    fn getprop_and_names() {
        let out = "[ro.build.version.release]: [14]\n[ro.build.version.sdk]: [34]\n[ro.product.manufacturer]: [samsung]\n[ro.product.model]: [SM-S911B]\n[other]: [x]\n";
        let info = parse_getprop(out);
        assert_eq!(info.sdk, 34);
        assert_eq!(info.android_version, "14");
        assert_eq!(info.friendly_name(), "Samsung SM-S911B");
        let pixel = DeviceInfo {
            manufacturer: "Google".into(),
            model: "Pixel 7".into(),
            ..Default::default()
        };
        assert_eq!(pixel.friendly_name(), "Google Pixel 7");
        let xiaomi = DeviceInfo {
            manufacturer: "Xiaomi".into(),
            model: "Xiaomi 13".into(),
            ..Default::default()
        };
        assert_eq!(xiaomi.friendly_name(), "Xiaomi 13");
    }

    #[test]
    fn install_failures_are_explained() {
        assert!(
            install_problem("Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: ...]")
                .hint
                .unwrap()
                .contains("Uninstall")
        );
        assert!(install_problem(
            "Failure [INSTALL_FAILED_USER_RESTRICTED: Install canceled by user]"
        )
        .hint
        .unwrap()
        .contains("Xiaomi"));
    }
}
