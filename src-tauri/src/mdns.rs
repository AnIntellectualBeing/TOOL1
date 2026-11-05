//! mDNS discovery for Android wireless debugging, using the pure-Rust
//! `mdns-sd` crate (no Bonjour or system service needed).

use mdns_sd::{ServiceDaemon, ServiceEvent};
use phonemirror_core::discovery::{
    instance_from_fullname, Browse, Discovery, DiscoveryEvent, ServiceInstance,
};
use phonemirror_core::Problem;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Default)]
pub struct MdnsDiscovery {
    daemon: Mutex<Option<ServiceDaemon>>,
}

struct BrowseGuard {
    daemon: ServiceDaemon,
    service_type: String,
    stop: Arc<AtomicBool>,
}

impl Drop for BrowseGuard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.daemon.stop_browse(&self.service_type);
    }
}

impl MdnsDiscovery {
    fn daemon(&self) -> Result<ServiceDaemon, Problem> {
        let mut d = self.daemon.lock().unwrap();
        if d.is_none() {
            let daemon = ServiceDaemon::new().map_err(|e| {
                Problem::new("mdns_failed", "PhoneMirror couldn't listen for phones on the network.")
                    .hint("Another program may be blocking it. Restart PhoneMirror; if it continues, press \"Fix firewall\".")
                    .details(e.to_string())
            })?;
            *d = Some(daemon);
        }
        Ok(d.as_ref().unwrap().clone())
    }

    pub fn shutdown(&self) {
        if let Some(d) = self.daemon.lock().unwrap().take() {
            let _ = d.shutdown();
        }
    }
}

impl Discovery for MdnsDiscovery {
    fn browse(&self, service_type: &str) -> Result<Browse, Problem> {
        let daemon = self.daemon()?;
        let rx = daemon
            .browse(service_type)
            .map_err(|e| Problem::internal(format!("mdns browse {service_type}: {e}")))?;
        let (tx, events) = mpsc::unbounded_channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let st = service_type.to_string();
        std::thread::spawn(move || loop {
            if stop2.load(Ordering::SeqCst) || tx.is_closed() {
                break;
            }
            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(ServiceEvent::ServiceResolved(info)) => {
                    let inst = ServiceInstance {
                        instance: instance_from_fullname(info.get_fullname(), &st),
                        addresses: info
                            .get_addresses()
                            .iter()
                            .map(|a| a.to_ip_addr())
                            .collect(),
                        port: info.get_port(),
                    };
                    log::debug!(
                        "mdns resolved {} {:?}:{}",
                        inst.instance,
                        inst.addresses,
                        inst.port
                    );
                    let _ = tx.send(DiscoveryEvent::Resolved(inst));
                }
                Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
                    let _ = tx.send(DiscoveryEvent::Removed(instance_from_fullname(
                        &fullname, &st,
                    )));
                }
                Ok(_) => {}
                Err(mdns_sd::RecvTimeoutError::Timeout) => {}
                Err(_) => break,
            }
        });
        Ok(Browse::new(
            events,
            Some(Box::new(BrowseGuard {
                daemon,
                service_type: service_type.to_string(),
                stop,
            })),
        ))
    }
}

/// IPv4 addresses of this PC with their prefixes.
pub fn local_ifaces() -> Vec<phonemirror_core::net::LocalIface> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(v4) => Some(phonemirror_core::net::LocalIface::new(
                i.name,
                v4.ip,
                v4.prefixlen,
            )),
            _ => None,
        })
        .collect()
}
