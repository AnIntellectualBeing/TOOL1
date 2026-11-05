# Building PhoneMirror

## Windows installer (what CI does)

CI (`.github/workflows/build.yml`) is the reference build. On every push it
runs the tests, builds UxPlay with its GStreamer runtime, checks the bundle on
a clean `PATH`, and builds the NSIS installer (artifact
`PhoneMirror-windows-installer`). Pushing a tag like `v0.1.0` also publishes a
GitHub Release with the installer, `SHA256SUMS.txt` and the third-party
sources zip. The tag must match `version` in `src-tauri/tauri.conf.json`.

To do the same by hand on Windows 10/11:

1. Install [Rust](https://rustup.rs), [Node.js 22](https://nodejs.org),
   [MSYS2](https://www.msys2.org) and Git for Windows.
2. iPhone engine, in an **MSYS2 UCRT64** shell from the repository folder:
   ```bash
   pacman -S --needed git binutils mingw-w64-ucrt-x86_64-{gcc,cmake,ninja,pkgconf,libplist,openssl,gstreamer,gst-plugins-base,gst-plugins-good,gst-plugins-bad,gst-libav,glib-networking}
   scripts/build-uxplay-windows.sh
   ```
   Then in PowerShell: `./scripts/check-uxplay-bundle.ps1 engines/iphone`
3. Android engine, in Git Bash: `scripts/fetch-android-engine.sh`
4. App and installer:
   ```powershell
   npm ci
   npx tauri build --bundles nsis
   ```
   The installer is in `target\release\bundle\nsis\`.

For development: `npx tauri dev`.

## Linux / macOS (development)

The app runs on Linux for development (Windows-only features such as
mute/always-on-top of the engine windows and the firewall fixer are no-ops).
Install the Tauri prerequisites (on Debian/Ubuntu: `libwebkit2gtk-4.1-dev
libxdo-dev libssl-dev librsvg2-dev`), plus `adb`, `scrcpy` and `uxplay` on your
`PATH` (PhoneMirror falls back to them when `engines/` is empty). Note that
your distro's UxPlay may buffer its output; build it with
`scripts/patches/0001-unbuffered-output.patch` for live status updates.

```bash
cargo test -p phonemirror-core      # all logic tests (mock adb + mDNS)
npm ci && npx tauri dev
```

## Useful bits

* `ui/index.html` opened directly in a browser runs in **demo mode** with a
  fake backend (`ui/demo.js`); add `?scene=iphone-connected` or `?scene=android`.
* `cargo run -p mdns-probe -- _adb-tls-connect._tcp.local.` lists Android
  phones with Wireless debugging on; `... _airplay._tcp.local. PhoneMirror`
  checks the iPhone receiver is visible on the network.
* `cargo run -p phonemirror-core --example qr_svg` prints a pairing QR.
* Engine versions are pinned in `scripts/engines.env`.
* `python3 scripts/rust-licenses.py` regenerates the Rust licence table in
  `THIRD_PARTY_LICENSES.md`.
