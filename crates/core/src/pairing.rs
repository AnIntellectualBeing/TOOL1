//! Wireless pairing and reconnection.
//!
//! QR flow, as Android Studio does it:
//! 1. Show `WIFI:T:ADB;S:<name>;P:<password>;;` as a QR code.
//! 2. The phone scans it and advertises `_adb-tls-pairing._tcp` named `<name>`.
//! 3. We find it over mDNS and run `adb pair <ip>:<port> <password>`.
//! 4. The phone's `_adb-tls-connect._tcp` service (a different port) is found
//!    and we run `adb connect <ip>:<port>`.
//! 5. The phone is remembered by its connect-service name, so next time we
//!    find it even if its IP or port changed.

use crate::adb::{Adb, CommandRunner, DeviceState};
use crate::discovery::{
    wait_for, Discovery, ServiceInstance, ADB_CONNECT_SERVICE, ADB_PAIRING_SERVICE,
};
use crate::errors::Problem;
use crate::net::{
    choose_address, host_port, parse_host_port, usable_ifaces, AddressChoice, LocalIface,
};
use crate::qr::PairingCredentials;
use crate::store::RememberedDevice;
use serde::Serialize;
use std::net::IpAddr;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Timeouts {
    /// How long the QR stays valid while we wait for the phone to scan it.
    pub scan: Duration,
    /// After pairing, how long to wait for the phone's connect service.
    pub connect_service: Duration,
    /// When reconnecting a remembered phone, how long to look for it.
    pub reconnect: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            scan: Duration::from_secs(180),
            connect_service: Duration::from_secs(20),
            reconnect: Duration::from_secs(8),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum PairingStep {
    WaitingForScan,
    PhoneFound { address: String },
    Pairing,
    LookingForPhone,
    Connecting { address: String },
    ReadingInfo,
}

pub type Progress<'a> = &'a (dyn Fn(PairingStep) + Send + Sync);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Connected {
    /// Serial to pass to `adb -s` / `scrcpy -s`.
    pub serial: String,
    pub device: RememberedDevice,
}

fn network_check(ifaces: &[LocalIface]) -> Result<(), Problem> {
    if usable_ifaces(ifaces).is_empty() {
        Err(Problem::no_network())
    } else {
        Ok(())
    }
}

/// Full QR pairing: wait for the scan, pair, connect, read the phone's name.
pub async fn pair_with_qr(
    runner: &dyn CommandRunner,
    discovery: &dyn Discovery,
    creds: &PairingCredentials,
    ifaces: &[LocalIface],
    timeouts: &Timeouts,
    now: u64,
    progress: Progress<'_>,
) -> Result<Connected, Problem> {
    network_check(ifaces)?;
    progress(PairingStep::WaitingForScan);
    let mut browse = discovery.browse(ADB_PAIRING_SERVICE)?;
    let wanted = creds.service_name.to_lowercase();
    let waited = wait_for(&mut browse, timeouts.scan, |i| {
        i.instance.to_lowercase() == wanted
    })
    .await;
    drop(browse);
    let Some(svc) = waited.found else {
        return Err(Problem::pairing_not_seen(timeouts.scan.as_secs()));
    };

    let (ip, different) = match choose_address(&svc.addresses, ifaces) {
        AddressChoice::SameNetwork(ip) => (ip, false),
        AddressChoice::DifferentNetwork(ip) => (ip, true),
        AddressChoice::None => return Err(Problem::network_blocks_devices()),
    };
    progress(PairingStep::PhoneFound {
        address: ip.to_string(),
    });
    progress(PairingStep::Pairing);
    let adb = Adb::new(runner);
    let paired = match adb.pair(&host_port(ip, svc.port), &creds.password).await {
        Ok(p) => p,
        Err(e) if different => {
            return Err(Problem::different_network(&ip.to_string()).details(e.to_string()))
        }
        Err(e) => return Err(e),
    };
    finish_after_pairing(
        runner,
        discovery,
        ip,
        paired.guid,
        ifaces,
        timeouts,
        now,
        progress,
    )
    .await
}

/// Manual fallback: the phone's "Pair device with pairing code" screen shows
/// an `ip:port` and a 6-digit code that the person types in.
#[allow(clippy::too_many_arguments)]
pub async fn pair_with_code(
    runner: &dyn CommandRunner,
    discovery: &dyn Discovery,
    pair_address: &str,
    code: &str,
    ifaces: &[LocalIface],
    timeouts: &Timeouts,
    now: u64,
    progress: Progress<'_>,
) -> Result<Connected, Problem> {
    network_check(ifaces)?;
    let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let (ip, port) = parse_host_port(pair_address).ok_or_else(|| {
        Problem::new("bad_address", "That address doesn't look right.")
            .hint("Type it exactly as the phone shows it, for example 192.168.1.23:37099.")
    })?;
    if let AddressChoice::DifferentNetwork(_) = choose_address(&[ip], ifaces) {
        return Err(Problem::different_network(&ip.to_string()));
    }
    progress(PairingStep::Pairing);
    let paired = Adb::new(runner).pair(&host_port(ip, port), &code).await?;
    finish_after_pairing(
        runner,
        discovery,
        ip,
        paired.guid,
        ifaces,
        timeouts,
        now,
        progress,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn finish_after_pairing(
    runner: &dyn CommandRunner,
    discovery: &dyn Discovery,
    phone_ip: IpAddr,
    guid: Option<String>,
    ifaces: &[LocalIface],
    timeouts: &Timeouts,
    now: u64,
    progress: Progress<'_>,
) -> Result<Connected, Problem> {
    progress(PairingStep::LookingForPhone);
    let adb = Adb::new(runner);
    let mut browse = discovery.browse(ADB_CONNECT_SERVICE)?;
    let found = wait_for(&mut browse, timeouts.connect_service, |i| match &guid {
        Some(g) => i.instance.eq_ignore_ascii_case(g),
        None => i.addresses.contains(&phone_ip),
    })
    .await
    .found;
    drop(browse);

    let serial = match found {
        Some(svc) => connect_to_service(&adb, &svc, ifaces, "your phone", progress).await?,
        None => {
            // adb's own mDNS may already have connected it.
            match guid
                .as_deref()
                .and_then(|g| Some(g).filter(|g| !g.is_empty()))
            {
                Some(g) => already_connected_serial(&adb, g, None)
                    .await?
                    .ok_or_else(|| Problem::phone_not_visible("your phone"))?,
                None => return Err(Problem::phone_not_visible("your phone")),
            }
        }
    };

    progress(PairingStep::ReadingInfo);
    let info = adb.device_info(&serial).await?;
    let id = guid.clone().unwrap_or_else(|| format!("wifi:{phone_ip}"));
    let device = RememberedDevice {
        id,
        name: info.friendly_name(),
        manufacturer: info.manufacturer.clone(),
        model: info.model.clone(),
        android_version: info.android_version.clone(),
        sdk: info.sdk,
        mdns_instance: guid,
        last_address: parse_host_port(&serial).map(|(i, p)| host_port(i, p)),
        usb_serial: None,
        paired_at: now,
        last_connected: now,
    };
    Ok(Connected { serial, device })
}

async fn connect_to_service(
    adb: &Adb<'_>,
    svc: &ServiceInstance,
    ifaces: &[LocalIface],
    name: &str,
    progress: Progress<'_>,
) -> Result<String, Problem> {
    let (ip, different) = match choose_address(&svc.addresses, ifaces) {
        AddressChoice::SameNetwork(ip) => (ip, false),
        AddressChoice::DifferentNetwork(ip) => (ip, true),
        AddressChoice::None => return Err(Problem::network_blocks_devices()),
    };
    let target = host_port(ip, svc.port);
    progress(PairingStep::Connecting {
        address: target.clone(),
    });
    match adb.connect(&target, name).await {
        Ok(_) => Ok(target),
        Err(e) if different && e.code != "not_paired" => {
            Err(Problem::different_network(&ip.to_string()).details(e.to_string()))
        }
        Err(e) => Err(e),
    }
}

/// If adb already has this phone connected (by mDNS name or last address),
/// return its serial.
async fn already_connected_serial(
    adb: &Adb<'_>,
    mdns_instance: &str,
    last_address: Option<&str>,
) -> Result<Option<String>, Problem> {
    let devices = adb.devices().await?;
    Ok(devices
        .into_iter()
        .filter(|d| d.state == DeviceState::Device)
        .find(|d| {
            d.mdns_instance()
                .is_some_and(|m| m.eq_ignore_ascii_case(mdns_instance))
                || last_address.is_some_and(|a| d.serial == a)
        })
        .map(|d| d.serial))
}

/// Reconnect a remembered phone over Wi-Fi.
pub async fn connect_remembered(
    runner: &dyn CommandRunner,
    discovery: &dyn Discovery,
    device: &RememberedDevice,
    ifaces: &[LocalIface],
    timeouts: &Timeouts,
    now: u64,
    progress: Progress<'_>,
) -> Result<Connected, Problem> {
    let adb = Adb::new(runner);
    let mdns = device.mdns_instance.clone().unwrap_or_default();

    if let Some(serial) =
        already_connected_serial(&adb, &mdns, device.last_address.as_deref()).await?
    {
        return Ok(updated(device, serial, now));
    }
    network_check(ifaces)?;

    progress(PairingStep::LookingForPhone);
    let mut seen_elsewhere: Option<IpAddr> = None;
    if !mdns.is_empty() {
        let mut browse = discovery.browse(ADB_CONNECT_SERVICE)?;
        let found = wait_for(&mut browse, timeouts.reconnect, |i| {
            i.instance.eq_ignore_ascii_case(&mdns)
        })
        .await
        .found;
        drop(browse);
        if let Some(svc) = found {
            match connect_to_service(&adb, &svc, ifaces, &device.name, progress).await {
                Ok(serial) => return Ok(updated(device, serial, now)),
                Err(e) if e.code == "different_network" => {
                    seen_elsewhere = svc.addresses.iter().find(|a| a.is_ipv4()).copied();
                }
                Err(e) => return Err(e),
            }
        }
    }

    // Fallback: last known address. Works when Wireless debugging wasn't
    // restarted (same port), or for phones set to a fixed `adb tcpip` port.
    if let Some(addr) = &device.last_address {
        progress(PairingStep::Connecting {
            address: addr.clone(),
        });
        if adb.connect(addr, &device.name).await.is_ok() {
            return Ok(updated(device, addr.clone(), now));
        }
        if let Some((ip, _)) = parse_host_port(addr) {
            if seen_elsewhere.is_none() {
                if let AddressChoice::DifferentNetwork(_) = choose_address(&[ip], ifaces) {
                    // Phone was last seen on another network and isn't visible here.
                    return Err(Problem::phone_not_visible(&device.name).hint(format!(
                        "Last time it was on a different network ({ip}). Connect the phone to \
                         this PC's Wi-Fi and check Wireless debugging is on."
                    )));
                }
            }
        }
    }
    if let Some(ip) = seen_elsewhere {
        return Err(Problem::different_network(&ip.to_string()));
    }
    Err(Problem::phone_not_visible(&device.name))
}

fn updated(device: &RememberedDevice, serial: String, now: u64) -> Connected {
    let mut d = device.clone();
    if let Some((ip, port)) = parse_host_port(&serial) {
        d.last_address = Some(host_port(ip, port));
    }
    d.last_connected = now;
    Connected { serial, device: d }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb::{CmdOutput, RunError};
    use crate::discovery::mock::{inst, MockDiscovery};
    use async_trait::async_trait;
    use std::net::Ipv4Addr;
    use std::sync::Mutex;

    /// Fake adb: answers by matching the start of the argument list.
    #[derive(Default)]
    struct MockAdb {
        answers: Vec<(Vec<String>, CmdOutput)>,
        calls: Mutex<Vec<Vec<String>>>,
    }

    impl MockAdb {
        fn on(mut self, prefix: &[&str], out: CmdOutput) -> Self {
            self.answers
                .push((prefix.iter().map(|s| s.to_string()).collect(), out));
            self
        }
        fn calls(&self) -> Vec<String> {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .map(|c| c.join(" "))
                .collect()
        }
    }

    #[async_trait]
    impl CommandRunner for MockAdb {
        async fn run(&self, args: &[String], _t: Duration) -> Result<CmdOutput, RunError> {
            self.calls.lock().unwrap().push(args.to_vec());
            for (prefix, out) in &self.answers {
                if args.len() >= prefix.len() && args[..prefix.len()] == prefix[..] {
                    return Ok(out.clone());
                }
            }
            Ok(CmdOutput::fail(
                1,
                format!("unexpected: {}", args.join(" ")),
            ))
        }
    }

    const GETPROP: &str = "[ro.product.manufacturer]: [Google]\n[ro.product.model]: [Pixel 7]\n[ro.build.version.release]: [14]\n[ro.build.version.sdk]: [34]\n";
    const GUID: &str = "adb-28041FDH2000DP-aBcDeF";

    fn home() -> Vec<LocalIface> {
        vec![LocalIface::new("Wi-Fi", Ipv4Addr::new(192, 168, 1, 20), 24)]
    }

    fn fast() -> Timeouts {
        Timeouts {
            scan: Duration::from_millis(300),
            connect_service: Duration::from_millis(300),
            reconnect: Duration::from_millis(200),
        }
    }

    fn creds() -> PairingCredentials {
        PairingCredentials {
            service_name: "phonemirror-Ab12Cd".into(),
            password: "pw123456789x".into(),
        }
    }

    fn steps() -> (
        std::sync::Arc<Mutex<Vec<PairingStep>>>,
        impl Fn(PairingStep) + Send + Sync,
    ) {
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        let l2 = log.clone();
        (log, move |s| l2.lock().unwrap().push(s))
    }

    #[tokio::test]
    async fn qr_pairing_happy_path() {
        let adb = MockAdb::default()
            .on(
                &["pair"],
                CmdOutput::ok(format!(
                    "Successfully paired to 192.168.1.50:37099 [guid={GUID}]"
                )),
            )
            .on(
                &["connect"],
                CmdOutput::ok("connected to 192.168.1.50:41235"),
            )
            .on(
                &["-s", "192.168.1.50:41235", "shell", "getprop"],
                CmdOutput::ok(GETPROP),
            );
        let disc = MockDiscovery::default()
            // Another phone pairing nearby must be ignored.
            .on(
                ADB_PAIRING_SERVICE,
                10,
                inst("studio-zzz", "192.168.1.77", 1111),
            )
            .on(
                ADB_PAIRING_SERVICE,
                10,
                inst("phonemirror-Ab12Cd", "192.168.1.50", 37099),
            )
            .on(
                ADB_CONNECT_SERVICE,
                10,
                inst("adb-OTHER-1", "192.168.1.77", 2222),
            )
            .on(ADB_CONNECT_SERVICE, 10, inst(GUID, "192.168.1.50", 41235));
        let (log, progress) = steps();
        let c = pair_with_qr(&adb, &disc, &creds(), &home(), &fast(), 1000, &progress)
            .await
            .unwrap();

        assert_eq!(c.serial, "192.168.1.50:41235");
        assert_eq!(c.device.id, GUID);
        assert_eq!(c.device.name, "Google Pixel 7");
        assert_eq!(c.device.sdk, 34);
        assert_eq!(c.device.last_address.as_deref(), Some("192.168.1.50:41235"));
        assert_eq!(c.device.paired_at, 1000);
        assert_eq!(
            adb.calls(),
            [
                "pair 192.168.1.50:37099 pw123456789x",
                "connect 192.168.1.50:41235",
                "-s 192.168.1.50:41235 shell getprop"
            ]
        );
        let log = log.lock().unwrap();
        assert_eq!(log[0], PairingStep::WaitingForScan);
        assert!(log.contains(&PairingStep::Pairing));
        assert!(log.contains(&PairingStep::ReadingInfo));
    }

    #[tokio::test]
    async fn qr_pairing_not_scanned_times_out() {
        let adb = MockAdb::default();
        let disc = MockDiscovery::default().on(
            ADB_PAIRING_SERVICE,
            5,
            inst("studio-zzz", "192.168.1.77", 1),
        );
        let (_, progress) = steps();
        let err = pair_with_qr(&adb, &disc, &creds(), &home(), &fast(), 0, &progress)
            .await
            .unwrap_err();
        assert_eq!(err.code, "pairing_timeout");
        assert!(
            adb.calls().is_empty(),
            "must not call adb pair for someone else's phone"
        );
    }

    #[tokio::test]
    async fn qr_pairing_on_different_network() {
        let adb = MockAdb::default().on(
            &["pair"],
            CmdOutput::fail(1, "Failed: Unable to start pairing client."),
        );
        let disc = MockDiscovery::default().on(
            ADB_PAIRING_SERVICE,
            5,
            inst("phonemirror-Ab12Cd", "10.20.0.9", 37099),
        );
        let (_, progress) = steps();
        let err = pair_with_qr(&adb, &disc, &creds(), &home(), &fast(), 0, &progress)
            .await
            .unwrap_err();
        assert_eq!(err.code, "different_network");
        assert_eq!(
            err.message,
            "Your phone and PC are on different Wi-Fi networks."
        );
    }

    #[tokio::test]
    async fn qr_pairing_wrong_password() {
        let adb = MockAdb::default().on(
            &["pair"],
            CmdOutput::fail(1, "Failed: Wrong password or connection was dropped."),
        );
        let disc = MockDiscovery::default().on(
            ADB_PAIRING_SERVICE,
            5,
            inst("phonemirror-Ab12Cd", "192.168.1.50", 37099),
        );
        let (_, progress) = steps();
        let err = pair_with_qr(&adb, &disc, &creds(), &home(), &fast(), 0, &progress)
            .await
            .unwrap_err();
        assert_eq!(err.code, "pairing_failed");
    }

    #[tokio::test]
    async fn no_network_fails_fast() {
        let adb = MockAdb::default();
        let disc = MockDiscovery::default();
        let lo = vec![LocalIface::new("lo", Ipv4Addr::LOCALHOST, 8)];
        let (_, progress) = steps();
        let err = pair_with_qr(&adb, &disc, &creds(), &lo, &fast(), 0, &progress)
            .await
            .unwrap_err();
        assert_eq!(err.code, "no_network");
        assert!(disc.browsed.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn pairing_falls_back_to_adb_auto_connect() {
        // Connect service never seen by us, but adb's own mDNS connected it.
        let adb = MockAdb::default()
            .on(
                &["pair"],
                CmdOutput::ok(format!(
                    "Successfully paired to 192.168.1.50:37099 [guid={GUID}]"
                )),
            )
            .on(
                &["devices", "-l"],
                CmdOutput::ok(format!(
                    "List of devices attached\n{GUID}._adb-tls-connect._tcp device model:Pixel_7\n"
                )),
            )
            .on(&["-s"], CmdOutput::ok(GETPROP));
        let disc = MockDiscovery::default().on(
            ADB_PAIRING_SERVICE,
            5,
            inst("phonemirror-Ab12Cd", "192.168.1.50", 37099),
        );
        let (_, progress) = steps();
        let c = pair_with_qr(&adb, &disc, &creds(), &home(), &fast(), 0, &progress)
            .await
            .unwrap();
        assert_eq!(c.serial, format!("{GUID}._adb-tls-connect._tcp"));
        assert_eq!(c.device.last_address, None);
    }

    #[tokio::test]
    async fn code_pairing() {
        let adb = MockAdb::default()
            .on(
                &["pair"],
                CmdOutput::ok("Successfully paired to 192.168.1.50:37099"),
            )
            .on(
                &["connect"],
                CmdOutput::ok("connected to 192.168.1.50:41235"),
            )
            .on(&["-s"], CmdOutput::ok(GETPROP));
        // Without a guid we match the connect service by IP address.
        let disc =
            MockDiscovery::default().on(ADB_CONNECT_SERVICE, 5, inst(GUID, "192.168.1.50", 41235));
        let (_, progress) = steps();
        let c = pair_with_code(
            &adb,
            &disc,
            " 192.168.1.50:37099 ",
            "123 456",
            &home(),
            &fast(),
            0,
            &progress,
        )
        .await
        .unwrap();
        assert_eq!(adb.calls()[0], "pair 192.168.1.50:37099 123456");
        assert_eq!(c.device.id, "wifi:192.168.1.50");
    }

    #[tokio::test]
    async fn code_pairing_rejects_garbage_address() {
        let (_, progress) = steps();
        let err = pair_with_code(
            &MockAdb::default(),
            &MockDiscovery::default(),
            "my phone",
            "1",
            &home(),
            &fast(),
            0,
            &progress,
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, "bad_address");
    }

    fn remembered() -> RememberedDevice {
        RememberedDevice {
            id: GUID.into(),
            name: "Google Pixel 7".into(),
            manufacturer: "Google".into(),
            model: "Pixel 7".into(),
            android_version: "14".into(),
            sdk: 34,
            mdns_instance: Some(GUID.into()),
            last_address: Some("192.168.1.50:41235".into()),
            usb_serial: None,
            paired_at: 1,
            last_connected: 1,
        }
    }

    #[tokio::test]
    async fn reconnect_finds_new_port_via_mdns() {
        let adb = MockAdb::default()
            .on(&["devices"], CmdOutput::ok("List of devices attached\n"))
            .on(
                &["connect", "192.168.1.61:39001"],
                CmdOutput::ok("connected to 192.168.1.61:39001"),
            );
        let disc =
            MockDiscovery::default().on(ADB_CONNECT_SERVICE, 5, inst(GUID, "192.168.1.61", 39001));
        let (_, progress) = steps();
        let c = connect_remembered(&adb, &disc, &remembered(), &home(), &fast(), 77, &progress)
            .await
            .unwrap();
        assert_eq!(c.serial, "192.168.1.61:39001");
        assert_eq!(c.device.last_address.as_deref(), Some("192.168.1.61:39001"));
        assert_eq!(c.device.last_connected, 77);
    }

    #[tokio::test]
    async fn reconnect_uses_existing_connection() {
        let adb = MockAdb::default().on(
            &["devices"],
            CmdOutput::ok(format!(
                "List of devices attached\n{GUID}._adb-tls-connect._tcp device model:Pixel_7\n"
            )),
        );
        let (_, progress) = steps();
        let c = connect_remembered(
            &adb,
            &MockDiscovery::default(),
            &remembered(),
            &home(),
            &fast(),
            5,
            &progress,
        )
        .await
        .unwrap();
        assert_eq!(c.serial, format!("{GUID}._adb-tls-connect._tcp"));
        assert_eq!(adb.calls(), ["devices -l"]);
    }

    #[tokio::test]
    async fn reconnect_falls_back_to_last_address() {
        let adb = MockAdb::default()
            .on(&["devices"], CmdOutput::ok("List of devices attached\n"))
            .on(
                &["connect", "192.168.1.50:41235"],
                CmdOutput::ok("connected to 192.168.1.50:41235"),
            );
        let (_, progress) = steps();
        let c = connect_remembered(
            &adb,
            &MockDiscovery::default(),
            &remembered(),
            &home(),
            &fast(),
            5,
            &progress,
        )
        .await
        .unwrap();
        assert_eq!(c.serial, "192.168.1.50:41235");
    }

    #[tokio::test]
    async fn reconnect_wireless_debugging_off() {
        let adb = MockAdb::default()
            .on(&["devices"], CmdOutput::ok("List of devices attached\n"))
            .on(
                &["connect"],
                CmdOutput::ok("failed to connect to '192.168.1.50:41235': Connection refused"),
            );
        let (_, progress) = steps();
        let err = connect_remembered(
            &adb,
            &MockDiscovery::default(),
            &remembered(),
            &home(),
            &fast(),
            5,
            &progress,
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, "phone_not_visible");
        assert!(err.hint.unwrap().contains("Wireless debugging"));
    }

    #[tokio::test]
    async fn reconnect_phone_on_other_network() {
        let adb = MockAdb::default()
            .on(&["devices"], CmdOutput::ok("List of devices attached\n"))
            .on(
                &["connect"],
                CmdOutput::ok("failed to connect to '172.16.5.5:40000': Connection timed out"),
            );
        let disc =
            MockDiscovery::default().on(ADB_CONNECT_SERVICE, 5, inst(GUID, "172.16.5.5", 40000));
        let (_, progress) = steps();
        let err = connect_remembered(&adb, &disc, &remembered(), &home(), &fast(), 5, &progress)
            .await
            .unwrap_err();
        assert_eq!(err.code, "different_network");
    }

    #[tokio::test]
    async fn reconnect_after_phone_forgot_pc() {
        let adb = MockAdb::default()
            .on(&["devices"], CmdOutput::ok("List of devices attached\n"))
            .on(
                &["connect"],
                CmdOutput::ok("failed to authenticate to 192.168.1.61:39001"),
            );
        let disc =
            MockDiscovery::default().on(ADB_CONNECT_SERVICE, 5, inst(GUID, "192.168.1.61", 39001));
        let (_, progress) = steps();
        let err = connect_remembered(&adb, &disc, &remembered(), &home(), &fast(), 5, &progress)
            .await
            .unwrap_err();
        assert_eq!(err.code, "not_paired");
    }

    #[tokio::test]
    async fn adb_missing_is_explained() {
        struct Missing;
        #[async_trait]
        impl CommandRunner for Missing {
            async fn run(&self, _a: &[String], _t: Duration) -> Result<CmdOutput, RunError> {
                Err(RunError::NotFound("C:/x/adb.exe".into()))
            }
        }
        let (_, progress) = steps();
        let err = connect_remembered(
            &Missing,
            &MockDiscovery::default(),
            &remembered(),
            &home(),
            &fast(),
            5,
            &progress,
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, "engine_missing");
    }
}
