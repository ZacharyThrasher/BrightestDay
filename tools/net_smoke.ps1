<#
.SYNOPSIS
  Brightest Day multiplayer smoke test: one headless host + N headless bot clients (+ optional
  windowed client), real sockets, scripted play (bots think on their replica world), terrain
  destruction. Asserts every client ends with the host's exact terrain hash and no desync, and
  reports RTT / bandwidth / corrections.

.EXAMPLE
  pwsh tools/net_smoke.ps1                          # LAN loopback, 3 clients, 60 s
  pwsh tools/net_smoke.ps1 -LagMs 40 -JitterMs 15   # through LagProxy (~80-110 ms RTT)
  pwsh tools/net_smoke.ps1 -Tunnel -Clients 2       # through a real cloudflared quick tunnel
  pwsh tools/net_smoke.ps1 -Windowed                # plus one real (small-window) game client
#>
param(
    [int]$Clients = 3,
    [int]$Secs = 60,
    [switch]$Tunnel,
    [double]$LagMs = 0,
    [double]$JitterMs = 0,
    [int]$Port = 27970,
    [int]$Bots = 2,
    [switch]$Windowed,
    [switch]$NoBuild,
    [string]$Profile = "release"
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR = Join-Path $root "target\cnet" }
$profDir = if ($Profile -eq "dev") { "debug" } else { $Profile }
$exe = Join-Path $env:CARGO_TARGET_DIR "$profDir\brightest_day.exe"
if (-not $NoBuild) {
    Write-Host "building brightest_day ($Profile) ..."
    Push-Location $root
    try { cargo build -p bd_game --profile $Profile 2>&1 | Select-Object -Last 3 | Out-Host } finally { Pop-Location }
    if ($LASTEXITCODE -ne 0) { throw "build failed" }
}
if (-not (Test-Path $exe)) { throw "missing $exe" }

$logDir = Join-Path $env:CARGO_TARGET_DIR "net_smoke"
New-Item -ItemType Directory -Force $logDir | Out-Null
Get-ChildItem $logDir -Filter *.log -ErrorAction SilentlyContinue | Remove-Item -Force
$procs = @()

function Start-Game([string]$name, [string[]]$argv) {
    $out = Join-Path $logDir "$name.out.log"
    $err = Join-Path $logDir "$name.err.log"
    $p = Start-Process -FilePath $exe -ArgumentList $argv -RedirectStandardOutput $out -RedirectStandardError $err -PassThru -WindowStyle Hidden -WorkingDirectory $root
    $script:procs += $p
    return @{ proc = $p; out = $out; err = $err; name = $name }
}

function Wait-Line([string]$file, [string]$pattern, [double]$timeout) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    while ($sw.Elapsed.TotalSeconds -lt $timeout) {
        if (Test-Path $file) {
            $m = Select-String -Path $file -Pattern $pattern -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($m) { return $m.Line }
        }
        Start-Sleep -Milliseconds 100
    }
    return $null
}

function Get-Stats($g) {
    $line = Select-String -Path $g.out -Pattern '^NETSTATS ' -ErrorAction SilentlyContinue | Select-Object -Last 1
    if (-not $line) { return $null }
    return ($line.Line.Substring(9) | ConvertFrom-Json)
}

try {
    $hostArgs = @("--host", "--headless", "--port", "$Port", "--quit-after", "$Secs", "--net-check-secs", "2", "--net-bots", "$Bots", "--seed", "7")
    if (-not $Tunnel) { $hostArgs += "--no-tunnel" }
    $h = Start-Game "host" $hostArgs
    $hl = Wait-Line $h.out '^NETHOST ' 60
    if (-not $hl) { throw "host did not start (see $($h.err))" }
    $realPort = [regex]::Match($hl, 'port=(\d+)').Groups[1].Value
    Write-Host $hl
    $addr = "127.0.0.1:$realPort"
    if ($Tunnel) {
        $cl = Wait-Line $h.out '^NETCODE ' 60
        Write-Host $cl
        if (-not $cl -or $cl -match 'failed') { throw "tunnel failed: $cl" }
        $addr = [regex]::Match($cl, 'code=(\S+)').Groups[1].Value
    }
    Write-Host "clients join '$addr'" $(if ($LagMs -gt 0) { "via LagProxy $LagMs ms + $JitterMs ms jitter each way" })

    $cls = @()
    for ($i = 1; $i -le $Clients; $i++) {
        $a = @("--join", $addr, "--headless", "--quit-after", "$Secs", "--name", "bot$i", "--seed", "$($i + 100)")
        if ($LagMs -gt 0 -or $JitterMs -gt 0) { $a += @("--lag-ms", "$LagMs", "--jitter-ms", "$JitterMs") }
        $cls += Start-Game "client$i" $a
        Start-Sleep -Milliseconds 700
    }
    if ($Windowed) {
        $w = Start-Game "windowed" @("--join", $addr, "--name", "window", "--windowed", "--size", "640x360", "--quit-after", "$($Secs - 5)")
    }
    foreach ($c in $cls) { $c.proc | Wait-Process -Timeout ($Secs + 90) -ErrorAction SilentlyContinue }
    $h.proc | Wait-Process -Timeout 60 -ErrorAction SilentlyContinue

    $hs = Get-Stats $h
    if (-not $hs) { throw "host printed no NETSTATS (see $($h.err))" }
    Write-Host ("host  terrain {0}  tx {1} kbps  rx {2} kbps  checks {3}  inits {4}  resyncs {5}  late-input ticks {6}  tick p99 {7} ms  net {8} ms/tick-frame" -f $hs.terrain_hash, $hs.tx_kbps, $hs.rx_kbps, $hs.terrain_checks, $hs.inits, $hs.resyncs, $hs.input_starved, $hs.tick_p99_ms, $hs.net_cost_ms)
    $fail = @()
    foreach ($c in $cls) {
        $s = Get-Stats $c
        if (-not $s) { $fail += "$($c.name): no NETSTATS"; continue }
        Write-Host ("{0}  terrain {1}  rtt {2} ms (min {3}, p95 {4}, jitter {5})  up {6} / down {7} kbps  corrections {8} (visible {9}, snapped {10}, max {11} m)  checks ok {12}  desyncs {13}  interp {14} ticks starved {15}  casts {16} ok / {17} failed  net {18} ms/frame" -f `
            $c.name, $s.terrain_hash, $s.rtt_ms, $s.rtt_min_ms, $s.rtt_p95_ms, $s.jitter_ms, $s.tx_kbps, $s.rx_kbps, $s.corrections, $s.visible_corrections, $s.snapped, $s.max_error_m, $s.terrain_checks_ok, $s.desyncs, $s.interp_delay_ticks, $s.interp_starved, $s.casts_ok, $s.casts_failed, $s.net_cost_ms)
        if (-not $s.joined) { $fail += "$($c.name): never joined" }
        if ($s.terrain_hash -ne $hs.terrain_hash) { $fail += "$($c.name): terrain hash $($s.terrain_hash) != host $($hs.terrain_hash)" }
        if ($s.desyncs -gt 0) { $fail += "$($c.name): $($s.desyncs) desyncs" }
        if ($s.terrain_checks_ok -lt 3) { $fail += "$($c.name): only $($s.terrain_checks_ok) terrain checks" }
    }
    if ($fail.Count) {
        $fail | ForEach-Object { Write-Host "FAIL $_" -ForegroundColor Red }
        Write-Host "logs: $logDir"
        exit 1
    }
    Write-Host "PASS: $($cls.Count) clients, identical terrain ($($hs.terrain_hash)), no desync" -ForegroundColor Green
}
finally {
    foreach ($p in $procs) { if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue } }
    # cloudflared is in a kill-on-close job object of the host; make sure nothing we started lingers.
    Get-Process cloudflared -ErrorAction SilentlyContinue | Where-Object { $_.Parent.Id -in $procs.Id } | Stop-Process -Force -ErrorAction SilentlyContinue
}
