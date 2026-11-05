# engines/

Bundled third-party programs are placed here at build time and are **not**
committed to git. CI fills it; locally run the scripts in `scripts/`.

```
engines/
  android/   scrcpy.exe, scrcpy-server, adb.exe, AdbWinApi.dll, AdbWinUsbApi.dll, SDL/FFmpeg DLLs
  iphone/    uxplay.exe + GStreamer runtime (DLLs, lib/gstreamer-1.0/, libexec/)
```

When a folder is missing, PhoneMirror falls back to `adb`, `scrcpy` and
`uxplay` found on your PATH (useful for development on Linux or macOS).
