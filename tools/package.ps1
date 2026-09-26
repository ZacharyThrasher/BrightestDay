# Builds a self-contained Windows release of Brightest Day into dist\BrightestDay and zips it.
#
#   pwsh tools/package.ps1            # release build with push-to-talk voice (needs LLVM + CMake + MSVC)
#   pwsh tools/package.ps1 -NoVoice   # build without voice (no C++ toolchain needed)
#
# The game looks for assets/ next to its exe, so the folder is fully portable.
param([switch]$NoVoice, [string]$Out = "dist")

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$env:CARGO_TARGET_DIR = Join-Path $root "target\package"

if (-not (Test-Path "assets\lexicon.bin")) {
    Write-Host "assets\lexicon.bin missing - building it (one-time, ~6 min)..."
    python tools/lexicon_build/build.py
}

$features = if ($NoVoice) { @() } else { @("--features", "voice") }
Write-Host "cargo build --release -p bd_game $features"
cargo build --release -p bd_game @features
if ($LASTEXITCODE -ne 0) {
    if ($NoVoice) { throw "build failed" }
    Write-Warning "Voice build failed; retrying without voice."
    cargo build --release -p bd_game
    if ($LASTEXITCODE -ne 0) { throw "build failed" }
}

$dist = Join-Path $root "$Out\BrightestDay"
if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
New-Item -ItemType Directory -Force $dist | Out-Null
Copy-Item (Join-Path $env:CARGO_TARGET_DIR "release\brightest_day.exe") $dist

# Assets (skip dev-only files).
$exclude = @("lexicon_preview.json", "powers_pending")
New-Item -ItemType Directory -Force "$dist\assets" | Out-Null
Get-ChildItem assets | Where-Object { $exclude -notcontains $_.Name } | ForEach-Object {
    Copy-Item $_.FullName "$dist\assets" -Recurse
}

# cloudflared (Apache-2.0) for one-click hosting: bundled next to the exe if installed on this machine.
$cf = @((Get-Command cloudflared -ErrorAction SilentlyContinue).Source, "C:\Program Files (x86)\cloudflared\cloudflared.exe",
        "C:\Program Files\cloudflared\cloudflared.exe") | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if ($cf) { Copy-Item $cf $dist; Write-Host "bundled cloudflared from $cf" }
else { Write-Warning "cloudflared not found - online hosting needs it installed (LAN play still works)." }

Copy-Item "docs\PLAYING.md" "$dist\README.md"
Copy-Item "THIRD_PARTY_NOTICES.md" $dist

$zip = Join-Path $root "$Out\BrightestDay-win64.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path $dist -DestinationPath $zip -CompressionLevel Optimal
$mb = [math]::Round((Get-Item $zip).Length / 1MB, 1)
Write-Host "OK: $zip ($mb MB)"
