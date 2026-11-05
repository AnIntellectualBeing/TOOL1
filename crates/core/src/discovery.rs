//! mDNS (DNS-SD) discovery, abstracted so pairing logic can be tested without
//! a network. The app implements [`Discovery`] with the `mdns-sd` crate.

use crate::errors::Problem;
use std::net::IpAddr;
use std::time::Duration;
use tokio::sync::mpsc;

/// Advertised by a phone while its "Pair device with QR code" screen is open.
pub const ADB_PAIRING_SERVICE: &str = "_adb-tls-pairing._tcp.local.";
/// Advertised by a phone while Wireless debugging is on.
pub const ADB_CONNECT_SERVICE: &str = "_adb-tls-connect._tcp.local.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceInstance {
    /// Instance label only, e.g. `adb-28041FDH2000DP-aBcDeF`.
    pub instance: String,
    pub addresses: Vec<IpAddr>,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryEvent {
    Resolved(ServiceInstance),
    Removed(String),
}

/// A running browse. Dropping it stops browsing.
pub struct Browse {
    pub events: mpsc::UnboundedReceiver<DiscoveryEvent>,
    _guard: Option<Box<dyn Send + Sync>>,
}

impl Browse {
    pub fn new(
        events: mpsc::UnboundedReceiver<DiscoveryEvent>,
        guard: Option<Box<dyn Send + Sync>>,
    ) -> Self {
        Self {
            events,
            _guard: guard,
        }
    }
}

pub trait Discovery: Send + Sync {
    fn browse(&self, service_type: &str) -> Result<Browse, Problem>;
}

/// `phonemirror-x._adb-tls-pairing._tcp.local.` → `phonemirror-x`.
pub fn instance_from_fullname(fullname: &str, service_type: &str) -> String {
    let st = service_type.trim_end_matches('.');
    let full = fullname.trim_end_matches('.');
    full.strip_suffix(st)
        .map(|s| s.trim_end_matches('.'))
        .unwrap_or(full)
        .replace("\\032", " ")
        .replace("\\ ", " ")
}

#[derive(Debug, Default)]
pub struct WaitResult {
    pub found: Option<ServiceInstance>,
    /// Everything resolved while waiting, for diagnostics ("we saw other phones
    /// but not yours").
    pub seen: Vec<ServiceInstance>,
}

/// Wait until an instance matching `pred` resolves, or `timeout` passes.
pub async fn wait_for(
    browse: &mut Browse,
    timeout: Duration,
    mut pred: impl FnMut(&ServiceInstance) -> bool,
) -> WaitResult {
    let mut result = WaitResult::default();
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match tokio::time::timeout_at(deadline, browse.events.recv()).await {
            Ok(Some(DiscoveryEvent::Resolved(inst))) => {
                if !inst.addresses.is_empty() && pred(&inst) {
                    result.found = Some(inst);
                    return result;
                }
                result.seen.push(inst);
            }
            Ok(Some(DiscoveryEvent::Removed(_))) => {}
            // Channel closed (browser died) or timed out.
            Ok(None) | Err(_) => return result,
        }
    }
}

#[cfg(test)]
pub mod mock {
    //! A scripted discovery backend for tests.
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct MockDiscovery {
        /// Per service type: (delay before event, event).
        pub script: Mutex<HashMap<String, Vec<(Duration, DiscoveryEvent)>>>,
        pub browsed: Mutex<Vec<String>>,
    }

    impl MockDiscovery {
        pub fn on(self, service: &str, delay_ms: u64, inst: ServiceInstance) -> Self {
            self.script
                .lock()
                .unwrap()
                .entry(service.to_string())
                .or_default()
                .push((
                    Duration::from_millis(delay_ms),
                    DiscoveryEvent::Resolved(inst),
                ));
            self
        }
    }

    impl Discovery for MockDiscovery {
        fn browse(&self, service_type: &str) -> Result<Browse, Problem> {
            self.browsed.lock().unwrap().push(service_type.to_string());
            let (tx, rx) = mpsc::unbounded_channel();
            let events = self
                .script
                .lock()
                .unwrap()
                .get(service_type)
                .cloned()
                .unwrap_or_default();
            tokio::spawn(async move {
                for (delay, ev) in events {
                    tokio::time::sleep(delay).await;
                    if tx.send(ev).is_err() {
                        return;
                    }
                }
                // Keep the channel open like a real browser would.
                tokio::time::sleep(Duration::from_secs(3600)).await;
                drop(tx);
            });
            Ok(Browse::new(rx, None))
        }
    }

    pub fn inst(name: &str, ip: &str, port: u16) -> ServiceInstance {
        ServiceInstance {
            instance: name.into(),
            addresses: vec![ip.parse().unwrap()],
            port,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mock::*;
    use super::*;

    #[test]
    fn fullname_to_instance() {
        assert_eq!(
            instance_from_fullname(
                "phonemirror-abc._adb-tls-pairing._tcp.local.",
                ADB_PAIRING_SERVICE
            ),
            "phonemirror-abc"
        );
        assert_eq!(
            instance_from_fullname("adb-XYZ-1._adb-tls-connect._tcp.local", ADB_CONNECT_SERVICE),
            "adb-XYZ-1"
        );
    }

    #[tokio::test]
    async fn waits_for_matching_instance() {
        let d = MockDiscovery::default()
            .on(
                ADB_PAIRING_SERVICE,
                5,
                inst("someone-else", "192.168.1.9", 1),
            )
            .on(ADB_PAIRING_SERVICE, 5, inst("mine", "192.168.1.10", 2));
        let mut b = d.browse(ADB_PAIRING_SERVICE).unwrap();
        let r = wait_for(&mut b, Duration::from_secs(2), |i| i.instance == "mine").await;
        assert_eq!(r.found.unwrap().port, 2);
        assert_eq!(r.seen.len(), 1);
    }

    #[tokio::test]
    async fn times_out() {
        let d = MockDiscovery::default();
        let mut b = d.browse(ADB_PAIRING_SERVICE).unwrap();
        let r = wait_for(&mut b, Duration::from_millis(30), |_| true).await;
        assert!(r.found.is_none());
    }
}
