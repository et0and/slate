# vcpkg enables Media Foundation in its Windows FFmpeg build. Slate only needs
# decoding and scaling, and the static FFmpeg objects otherwise leave Media
# Foundation GUIDs unresolved at link time. Copy the runner's port into a
# temporary overlay and turn that configure flag off.
$ErrorActionPreference = "Stop"

$source = Join-Path $env:VCPKG_INSTALLATION_ROOT "ports\ffmpeg"
$overlayRoot = Join-Path $env:RUNNER_TEMP "vcpkg-overlays"
$overlayPort = Join-Path $overlayRoot "ffmpeg"

if (-not (Test-Path $source)) {
    throw "vcpkg's FFmpeg port was not found at $source"
}
if (Test-Path $overlayPort) {
    Remove-Item -Path $overlayPort -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $overlayRoot | Out-Null
Copy-Item -Path $source -Destination $overlayRoot -Recurse -Force

$portfile = Join-Path $overlayPort "portfile.cmake"
$text = Get-Content -Path $portfile -Raw
if (-not $text.Contains("--enable-mediafoundation")) {
    throw "vcpkg's FFmpeg port no longer enables Media Foundation; remove this overlay."
}
$text = $text.Replace("--enable-mediafoundation", "--disable-mediafoundation")
Set-Content -Path $portfile -Value $text -NoNewline

$env:VCPKG_OVERLAY_PORTS = $overlayRoot
Write-Output "FFmpeg Media Foundation disabled in $overlayPort"
