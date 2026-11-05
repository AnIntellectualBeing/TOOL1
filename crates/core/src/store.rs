//! Small JSON files in the app's data folder: remembered phones and settings.
//! Writes are atomic (temp file + rename) so a crash can't leave half a file.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Load JSON from `path`. A missing file gives `T::default()`. A corrupt file
/// is moved aside to `<name>.corrupt` (so nothing is silently lost) and the
/// default is returned.
pub fn load_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    match fs::read(path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => {
                let _ = fs::rename(path, path.with_extension("corrupt"));
                T::default()
            }
        },
        Err(_) => T::default(),
    }
}

pub fn save_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let data = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)
}

/// A phone that was paired for wireless use (or seen over USB).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RememberedDevice {
    /// Stable id: the phone's `_adb-tls-connect` mDNS instance name when known
    /// (it survives IP and port changes), otherwise its USB serial.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub manufacturer: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub android_version: String,
    #[serde(default)]
    pub sdk: u32,
    /// `_adb-tls-connect` instance (e.g. `adb-28041FDH2000DP-aBcDeF`).
    #[serde(default)]
    pub mdns_instance: Option<String>,
    /// Last `ip:port` that worked; the port changes whenever Wireless
    /// debugging restarts, so this is only a fallback.
    #[serde(default)]
    pub last_address: Option<String>,
    #[serde(default)]
    pub usb_serial: Option<String>,
    #[serde(default)]
    pub paired_at: u64,
    #[serde(default)]
    pub last_connected: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DeviceList {
    #[serde(default)]
    pub devices: Vec<RememberedDevice>,
}

pub struct DeviceStore {
    path: PathBuf,
    list: DeviceList,
}

impl DeviceStore {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let list = load_json(&path);
        Self { path, list }
    }

    pub fn all(&self) -> Vec<RememberedDevice> {
        let mut v = self.list.devices.clone();
        v.sort_by_key(|d| std::cmp::Reverse(d.last_connected));
        v
    }

    pub fn get(&self, id: &str) -> Option<&RememberedDevice> {
        self.list.devices.iter().find(|d| d.id == id)
    }

    /// Insert or replace by id. Keeps the original `paired_at`.
    pub fn upsert(&mut self, mut device: RememberedDevice) -> io::Result<()> {
        if let Some(existing) = self.list.devices.iter_mut().find(|d| d.id == device.id) {
            if existing.paired_at != 0 {
                device.paired_at = existing.paired_at;
            }
            *existing = device;
        } else {
            self.list.devices.push(device);
        }
        self.save()
    }

    pub fn remove(&mut self, id: &str) -> io::Result<bool> {
        let before = self.list.devices.len();
        self.list.devices.retain(|d| d.id != id);
        let removed = self.list.devices.len() != before;
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    pub fn save(&self) -> io::Result<()> {
        save_json(&self.path, &self.list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(id: &str, last: u64) -> RememberedDevice {
        RememberedDevice {
            id: id.into(),
            name: format!("Phone {id}"),
            manufacturer: String::new(),
            model: String::new(),
            android_version: "14".into(),
            sdk: 34,
            mdns_instance: Some(id.into()),
            last_address: Some("192.168.1.2:4000".into()),
            usb_serial: None,
            paired_at: 100,
            last_connected: last,
        }
    }

    #[test]
    fn round_trip_and_ordering() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("devices.json");
        let mut s = DeviceStore::open(&path);
        assert!(s.all().is_empty());
        s.upsert(dev("a", 5)).unwrap();
        s.upsert(dev("b", 9)).unwrap();
        let s2 = DeviceStore::open(&path);
        let ids: Vec<_> = s2.all().into_iter().map(|d| d.id).collect();
        assert_eq!(ids, ["b", "a"]);
    }

    #[test]
    fn upsert_keeps_paired_at() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = DeviceStore::open(dir.path().join("d.json"));
        s.upsert(dev("a", 1)).unwrap();
        let mut newer = dev("a", 50);
        newer.paired_at = 999;
        newer.last_address = Some("10.0.0.2:5".into());
        s.upsert(newer).unwrap();
        let got = s.get("a").unwrap();
        assert_eq!(got.paired_at, 100);
        assert_eq!(got.last_address.as_deref(), Some("10.0.0.2:5"));
        assert_eq!(s.all().len(), 1);
    }

    #[test]
    fn remove_device() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = DeviceStore::open(dir.path().join("d.json"));
        s.upsert(dev("a", 1)).unwrap();
        assert!(s.remove("a").unwrap());
        assert!(!s.remove("a").unwrap());
        assert!(DeviceStore::open(dir.path().join("d.json"))
            .all()
            .is_empty());
    }

    #[test]
    fn corrupt_file_is_moved_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("d.json");
        fs::write(&path, b"{not json").unwrap();
        let s = DeviceStore::open(&path);
        assert!(s.all().is_empty());
        assert!(dir.path().join("d.corrupt").exists());
    }

    #[test]
    fn old_files_without_new_fields_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("d.json");
        fs::write(&path, br#"{"devices":[{"id":"x","name":"X"}]}"#).unwrap();
        let s = DeviceStore::open(&path);
        assert_eq!(s.get("x").unwrap().name, "X");
    }
}
