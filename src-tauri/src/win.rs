//! Window and audio control of the engine windows (scrcpy's and UxPlay's),
//! found by process id. Windows is implemented; other systems get safe
//! fallbacks so the code stays ready for macOS and Linux.

pub use imp::*;

#[cfg(windows)]
mod imp {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use windows::core::{Interface, BOOL};
    use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
        ISimpleAudioVolume, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId,
        IsWindowVisible, PostMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos,
        GWL_STYLE, GW_OWNER, HWND_NOTOPMOST, HWND_TOP, HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOMOVE,
        SWP_NOSIZE, SWP_NOZORDER, WM_CLOSE, WS_OVERLAPPEDWINDOW,
    };

    struct Search {
        pid: u32,
        found: Vec<isize>,
    }

    unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let s = &mut *(lparam.0 as *mut Search);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let owned = GetWindow(hwnd, GW_OWNER)
            .map(|o| !o.is_invalid())
            .unwrap_or(false);
        if pid == s.pid && IsWindowVisible(hwnd).as_bool() && !owned {
            s.found.push(hwnd.0 as isize);
        }
        BOOL(1)
    }

    /// Visible top-level windows of a process.
    pub fn windows_of(pid: u32) -> Vec<isize> {
        let mut s = Search { pid, found: vec![] };
        unsafe {
            let _ = EnumWindows(Some(enum_cb), LPARAM(&mut s as *mut Search as isize));
        }
        s.found
    }

    fn h(v: isize) -> HWND {
        HWND(v as *mut _)
    }

    pub fn request_close(pid: u32) {
        for w in windows_of(pid) {
            unsafe {
                let _ = PostMessageW(Some(h(w)), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }

    pub fn set_topmost(pid: u32, on: bool) -> bool {
        let wins = windows_of(pid);
        for w in &wins {
            unsafe {
                let after = if on { HWND_TOPMOST } else { HWND_NOTOPMOST };
                let _ = SetWindowPos(h(*w), Some(after), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
            }
        }
        !wins.is_empty()
    }

    static SAVED: Mutex<Option<HashMap<isize, (isize, RECT)>>> = Mutex::new(None);

    /// Borderless full screen on the window's monitor, or back. Returns the
    /// new state, or None if the window wasn't found.
    pub fn toggle_fullscreen(pid: u32) -> Option<bool> {
        let w = *windows_of(pid).first()?;
        let hwnd = h(w);
        let mut saved = SAVED.lock().unwrap();
        let map = saved.get_or_insert_with(HashMap::new);
        unsafe {
            if let Some((style, rect)) = map.remove(&w) {
                SetWindowLongPtrW(hwnd, GWL_STYLE, style);
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOP),
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_FRAMECHANGED | SWP_NOZORDER,
                );
                return Some(false);
            }
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
            let mut rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rect);
            map.insert(w, (style, rect));
            SetWindowLongPtrW(hwnd, GWL_STYLE, style & !(WS_OVERLAPPEDWINDOW.0 as isize));
            let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let _ = GetMonitorInfoW(mon, &mut mi);
            let r = mi.rcMonitor;
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOP),
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_FRAMECHANGED,
            );
            let _ = SetForegroundWindow(hwnd);
        }
        Some(true)
    }

    /// Mute or unmute a process in the Windows volume mixer (default output
    /// device). Returns false if the process has no audio session yet.
    pub fn set_muted(pid: u32, mute: bool) -> Result<bool, String> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let run = || -> windows::core::Result<bool> {
                let en: IMMDeviceEnumerator =
                    CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
                let dev = en.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
                let mgr: IAudioSessionManager2 = dev.Activate(CLSCTX_ALL, None)?;
                let sessions = mgr.GetSessionEnumerator()?;
                let mut found = false;
                for i in 0..sessions.GetCount()? {
                    let ctl = sessions.GetSession(i)?;
                    let ctl2: IAudioSessionControl2 = ctl.cast()?;
                    if ctl2.GetProcessId().unwrap_or(0) == pid {
                        let vol: ISimpleAudioVolume = ctl.cast()?;
                        vol.SetMute(mute, std::ptr::null())?;
                        found = true;
                    }
                }
                Ok(found)
            };
            run().map_err(|e| e.to_string())
        }
    }

    /// Screenshot of the process's largest window, as PNG.
    pub fn capture_window_png(pid: u32) -> Result<Vec<u8>, String> {
        let wins = xcap::Window::all().map_err(|e| e.to_string())?;
        let best = wins
            .into_iter()
            .filter(|w| w.pid().ok() == Some(pid) && !w.is_minimized().unwrap_or(true))
            .max_by_key(|w| w.width().unwrap_or(0) as u64 * w.height().unwrap_or(0) as u64)
            .ok_or_else(|| "window not found".to_string())?;
        let img = best.capture_image().map_err(|e| e.to_string())?;
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok(buf.into_inner())
    }
}

#[cfg(not(windows))]
#[allow(dead_code)]
mod imp {
    pub fn windows_of(_pid: u32) -> Vec<isize> {
        vec![]
    }
    pub fn request_close(pid: u32) {
        // scrcpy and UxPlay both shut down cleanly on SIGTERM.
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
    }
    pub fn set_topmost(_pid: u32, _on: bool) -> bool {
        false
    }
    pub fn toggle_fullscreen(_pid: u32) -> Option<bool> {
        None
    }
    pub fn set_muted(_pid: u32, _mute: bool) -> Result<bool, String> {
        Err("not supported on this system yet".into())
    }
    pub fn capture_window_png(_pid: u32) -> Result<Vec<u8>, String> {
        Err("not supported on this system yet".into())
    }
}
