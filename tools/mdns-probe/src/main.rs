//! `mdns-probe <service-type> [expect-substring] [seconds]`
//!
//! Browses for a DNS-SD service type and prints what it finds. With
//! `expect-substring`, exits 0 as soon as an instance whose name contains it
//! is resolved, or 1 after the timeout. Examples:
//!
//!   mdns-probe _airplay._tcp.local. PhoneMirror 20     # is the iPhone receiver visible?
//!   mdns-probe _adb-tls-connect._tcp.local.            # Android phones with Wireless debugging on
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::time::{Duration, Instant};

fn main() {
    let mut args = std::env::args().skip(1);
    let service = args.next().unwrap_or_else(|| "_airplay._tcp.local.".into());
    let expect = args.next();
    let secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(10);
    let daemon = ServiceDaemon::new().expect("start mDNS daemon");
    let rx = daemon.browse(&service).expect("browse");
    let deadline = Instant::now() + Duration::from_secs(secs);
    println!("Browsing {service} for {secs}s...");
    while Instant::now() < deadline {
        if let Ok(ServiceEvent::ServiceResolved(info)) = rx.recv_timeout(Duration::from_millis(200))
        {
            let addrs: Vec<String> = info
                .get_addresses()
                .iter()
                .map(|a| a.to_ip_addr().to_string())
                .collect();
            println!(
                "found: {} port {} addresses {:?}",
                info.get_fullname(),
                info.get_port(),
                addrs
            );
            if let Some(e) = &expect {
                if info.get_fullname().contains(e.as_str()) {
                    println!("OK: matched {e:?}");
                    let _ = daemon.shutdown();
                    std::process::exit(0);
                }
            }
        }
    }
    let _ = daemon.shutdown();
    if let Some(e) = expect {
        eprintln!("FAIL: no {service} instance containing {e:?} within {secs}s");
        std::process::exit(1);
    }
}
