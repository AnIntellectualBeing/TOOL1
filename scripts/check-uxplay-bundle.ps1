# Verify the bundled UxPlay runs on a "clean" Windows: only the bundle folder
# and System32 on PATH, only the bundled GStreamer plugins visible.
# Usage: pwsh scripts/check-uxplay-bundle.ps1 [engines\iphone]
param([string]$Bundle = "engines\iphone")
$ErrorActionPreference = "Stop"
$root = (Resolve-Path $Bundle).Path

$env:PATH = "$root;$env:SystemRoot\System32;$env:SystemRoot"
$env:GST_PLUGIN_SYSTEM_PATH_1_0 = "$root\lib\gstreamer-1.0"
$env:GST_PLUGIN_SYSTEM_PATH = "$root\lib\gstreamer-1.0"
$env:GST_PLUGIN_PATH_1_0 = ""
$env:GST_REGISTRY_1_0 = "$env:RUNNER_TEMP\gst-registry-check.bin"
$scanner = "$root\libexec\gstreamer-1.0\gst-plugin-scanner.exe"
if (Test-Path $scanner) { $env:GST_PLUGIN_SCANNER_1_0 = $scanner }
$env:GIO_MODULE_DIR = "$root\lib\gio\modules"

$failed = @()
# Elements UxPlay's pipelines use on Windows with PhoneMirror's options.
$elements = @(
  "appsrc", "queue", "decodebin", "playbin", "h264parse", "h265parse", "aacparse",
  "avdec_h264", "avdec_aac", "avdec_alac", "videoconvert", "videoscale", "videoflip",
  "audioconvert", "audioresample", "volume", "d3d11videosink", "d3d11h264dec",
  "wasapisink", "autoaudiosink", "autovideosink", "mp4mux", "filesink", "fakesink"
)
foreach ($e in $elements) {
  & "$root\gst-inspect-1.0.exe" --exists $e
  if ($LASTEXITCODE -ne 0) { $failed += $e; Write-Host "MISSING element: $e" } else { Write-Host "ok: $e" }
}
# Optional (HLS casting / D3D12) - report only.
foreach ($e in @("d3d12videosink", "hlsdemux2", "hlsdemux", "souphttpsrc")) {
  & "$root\gst-inspect-1.0.exe" --exists $e
  Write-Host ("optional {0}: {1}" -f $e, $(if ($LASTEXITCODE -eq 0) { "present" } else { "absent" }))
}

$help = & "$root\uxplay.exe" -h 2>&1 | Out-String
if ($help -notmatch "UxPlay") { $failed += "uxplay.exe -h"; Write-Host $help }
else { Write-Host ($help -split "`n")[0] }

if ($failed.Count -gt 0) { throw "Bundle check failed: $($failed -join ', ')" }
Write-Host "UxPlay bundle check passed."
