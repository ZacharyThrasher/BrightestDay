<#
.SYNOPSIS
    Brightest Day quality gate: the single command that decides whether something integrates.

.DESCRIPTION
    Stages (each prints PASS / FAIL / WARN / SKIP; the script exits 1 if any stage FAILs):

      manifest  workspace manifests parse and all targets exist (a broken in-progress crate is
                reported and the other crates are checked in an isolated mirror workspace)
      fmt       cargo fmt --check per crate (report-only unless a rustfmt.toml exists or -StrictFmt)
      clippy    cargo clippy per crate: compile errors / denied lints FAIL; panicking constructs
                (unwrap/expect/panic/indexing...) in runtime code are counted per crate and compared
                with tools/lint_budget.json (a crate listed there may never exceed its budget)
      test      cargo test per crate (unit + integration + property + doc tests), `gate` profile
                (optimised, with debug assertions and overflow checks ON)
      soak      -Soak <secs>: bd_sim chaos soak example, <secs> simulated seconds, random seed (printed)
      bench     -Bench: brightest_day --bench 20 (release); p99 frame time vs tools/perf_thresholds.ron
      smoke     -Smoke: brightest_day --screenshot <png> --quit-after 8 (release); PNG must exist and
                not be blank/black

    All builds use CARGO_TARGET_DIR=target\check so they never collide with other engineers' dirs.
    Full logs: target\check\logs\<stage>-<crate>.log

.EXAMPLE
    pwsh tools/check.ps1                              # fmt + clippy + test, every crate
    pwsh tools/check.ps1 -Crates bd_voxel,bd_net      # only these crates
    pwsh tools/check.ps1 -Soak 120 -Bench -Smoke      # the full integration gate
    pwsh tools/check.ps1 -Deep -Crates bd_net         # property tests with 10x the cases
#>
[CmdletBinding()]
param(
    # Restrict to these workspace members (default: all). Accepts "bd_sim" or "sim".
    [string[]]$Crates = @(),
    # Stages to skip: fmt, clippy, test (comma-separated or array).
    [string[]]$Skip = @(),
    # fmt diffs are reported but never fail the gate.
    [switch]$FmtReportOnly,
    # fmt diffs fail the gate even without a rustfmt.toml.
    [switch]$StrictFmt,
    # Any clippy warning fails the gate (-D warnings).
    [switch]$ClippyStrict,
    # Rewrite tools/lint_budget.json with the current counts for the crates checked.
    [switch]$UpdateLintBudget,
    # Cargo profile for tests: gate (default, optimised + overflow checks) or release/dev.
    [ValidateSet('gate', 'release', 'dev')][string]$CargoProfile = 'gate',
    # Property tests run BD_PROP_SCALE x their case counts (default 1; -Deep = 10).
    [switch]$Deep,
    [int]$PropScale = 0,
    # Chaos soak duration in seconds (0 = skip).
    [int]$Soak = 0,
    [UInt64]$SoakSeed = 0,
    [switch]$Bench,
    [int]$BenchSecs = 20,
    [switch]$Smoke,
    # Force the isolated mirror workspace even if the real one is healthy.
    [switch]$Isolated,
    # Per cargo invocation timeout (minutes).
    [int]$TimeoutMin = 60
)

Set-StrictMode -Version 3
$ErrorActionPreference = 'Stop'

$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Target = Join-Path $Root 'target\check'
$Logs = Join-Path $Target 'logs'
New-Item -ItemType Directory -Force -Path $Logs | Out-Null
$env:CARGO_TARGET_DIR = $Target
$env:CARGO_TERM_COLOR = 'never'
if ($Deep -and $PropScale -eq 0) { $PropScale = 10 }
if ($PropScale -gt 0) { $env:BD_PROP_SCALE = "$PropScale" } else { Remove-Item Env:BD_PROP_SCALE -ErrorAction SilentlyContinue }

# Runtime crates: code paths that handle untrusted data (network bytes, typed text, recipes, carve
# ops). Panicking constructs here are what "no panics on untrusted data" is about.
$RuntimeCrates = @('bd_core', 'bd_voxel', 'bd_sim', 'bd_net', 'bd_lexicon')
$PanicLints = @('clippy::unwrap_used', 'clippy::expect_used', 'clippy::panic', 'clippy::indexing_slicing',
    'clippy::unreachable', 'clippy::todo', 'clippy::unimplemented')

$Results = New-Object System.Collections.Generic.List[object]
$Started = Get-Date

function Write-Status([string]$Stage, [string]$Status, [string]$Detail = '') {
    $color = switch ($Status) { 'PASS' { 'Green' } 'FAIL' { 'Red' } 'WARN' { 'Yellow' } default { 'DarkGray' } }
    Write-Host ('  {0,-4} ' -f $Status) -ForegroundColor $color -NoNewline
    Write-Host ('{0,-26} {1}' -f $Stage, $Detail)
    $Results.Add([pscustomobject]@{ Stage = $Stage; Status = $Status; Detail = $Detail })
}

function Write-Section([string]$Title) {
    Write-Host ''
    Write-Host "== $Title" -ForegroundColor Cyan
}

# Run a native command with a timeout; stdout+stderr go to $Log. Returns the exit code (124 = timeout).
function Invoke-Logged([string]$Exe, [string[]]$Argv, [string]$Log, [string]$WorkDir = $Root, [int]$TimeoutSec = $TimeoutMin * 60) {
    $quoted = $Argv | ForEach-Object { if ($_ -match '[\s"]') { '"' + ($_ -replace '"', '\"') + '"' } else { $_ } }
    $err = "$Log.stderr"
    Set-Content -Path $Log -Value "> $Exe $($quoted -join ' ')`n" -Encoding utf8
    $p = Start-Process -FilePath $Exe -ArgumentList $quoted -WorkingDirectory $WorkDir -NoNewWindow -PassThru `
        -RedirectStandardOutput "$Log.stdout" -RedirectStandardError $err
    $null = $p.Handle # makes ExitCode available after exit (Start-Process quirk)
    if (-not $p.WaitForExit($TimeoutSec * 1000)) {
        & taskkill /T /F /PID $p.Id 2>&1 | Out-Null
        Add-Content -Path $Log -Value "`n!! TIMEOUT after $TimeoutSec s"
        $code = 124
    } else {
        $p.WaitForExit()
        $code = $p.ExitCode
    }
    foreach ($f in @("$Log.stdout", $err)) {
        if (Test-Path $f) { Get-Content $f -Raw -ErrorAction SilentlyContinue | Add-Content -Path $Log; Remove-Item $f -Force }
    }
    return [int]$code
}

function Show-Tail([string]$Log, [int]$Lines = 25, [string]$Pattern = '') {
    if (-not (Test-Path $Log)) { return }
    $content = Get-Content $Log
    if ($Pattern) {
        $hits = $content | Select-String -Pattern $Pattern | Select-Object -First $Lines
        foreach ($h in $hits) { Write-Host "         $($h.Line)" -ForegroundColor DarkYellow }
        if ($hits) { return }
    }
    $content | Select-Object -Last $Lines | ForEach-Object { Write-Host "         $_" -ForegroundColor DarkGray }
    Write-Host "         (full log: $Log)" -ForegroundColor DarkGray
}

# ------------------------------------------------------------------------------------ workspace model

function Get-Members {
    $toml = Get-Content (Join-Path $Root 'Cargo.toml') -Raw
    $m = [regex]::Match($toml, '(?s)members\s*=\s*\[(.*?)\]')
    [regex]::Matches($m.Groups[1].Value, '"crates/([^"]+)"') | ForEach-Object { $_.Groups[1].Value }
}

# bd_* path dependencies of a crate (normal + dev + build).
function Get-PathDeps([string]$Crate) {
    $manifest = Join-Path $Root "crates\$Crate\Cargo.toml"
    if (-not (Test-Path $manifest)) { return @() }
    Get-Content $manifest | ForEach-Object {
        if ($_ -match '^\s*(bd_[a-z0-9_]+)\s*(\.workspace\s*=\s*true|=)') { $Matches[1] }
    } | Where-Object { $_ -ne $Crate } | Sort-Object -Unique
}

function Get-Closure([string[]]$Names) {
    $seen = @{}
    $queue = New-Object System.Collections.Generic.Queue[string]
    foreach ($n in $Names) { $queue.Enqueue($n) }
    while ($queue.Count -gt 0) {
        $c = $queue.Dequeue()
        if ($seen.ContainsKey($c)) { continue }
        $seen[$c] = $true
        foreach ($d in (Get-PathDeps $c)) { $queue.Enqueue($d) }
    }
    $seen.Keys | Sort-Object
}

# Mirror the selected crates (+ path deps) into target\check\ws with a trimmed workspace manifest,
# so a crate whose manifest is broken (or mid-edit) can't block checking everything else.
function New-Mirror([string[]]$Names) {
    $ws = Join-Path $Target 'ws'
    $all = Get-Closure $Names
    New-Item -ItemType Directory -Force -Path (Join-Path $ws 'crates'), (Join-Path $ws '.cargo') | Out-Null
    foreach ($c in $all) {
        $src = Join-Path $Root "crates\$c"
        $dst = Join-Path $ws "crates\$c"
        & robocopy $src $dst /MIR /XD target proptest-regressions /NFL /NDL /NJH /NJS /NP | Out-Null
        if ($LASTEXITCODE -ge 8) { throw "robocopy failed for $c ($LASTEXITCODE)" }
    }
    $global:LASTEXITCODE = 0
    Copy-Item (Join-Path $Root 'Cargo.lock') (Join-Path $ws 'Cargo.lock') -Force
    $cfg = Join-Path $Root '.cargo\config.toml'
    if (Test-Path $cfg) { Copy-Item $cfg (Join-Path $ws '.cargo\config.toml') -Force }
    $toml = Get-Content (Join-Path $Root 'Cargo.toml') -Raw
    $members = ($all | ForEach-Object { "`"crates/$_`"" }) -join ', '
    $toml = [regex]::Replace($toml, '(?s)members\s*=\s*\[.*?\]', "members = [$members]")
    [System.IO.File]::WriteAllText((Join-Path $ws 'Cargo.toml'), $toml, (New-Object System.Text.UTF8Encoding($false)))
    # A real copy, not a junction: `Remove-Item -Recurse` in Windows PowerShell 5.1 follows junctions
    # and would delete the real assets when someone cleans target\. Whisper models are skipped (big,
    # and bd_voice degrades to its stub without them).
    & robocopy (Join-Path $Root 'assets') (Join-Path $ws 'assets') /MIR /XD models /NFL /NDL /NJH /NJS /NP | Out-Null
    if ($LASTEXITCODE -ge 8) { throw "robocopy failed for assets ($LASTEXITCODE)" }
    $global:LASTEXITCODE = 0
    return $ws
}

# ------------------------------------------------------------------------------------------ selection

$Skip = @($Skip | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim().ToLower() } | Where-Object { $_ })
foreach ($s in $Skip) { if (@('fmt', 'clippy', 'test') -notcontains $s) { throw "unknown stage to skip: '$s' (fmt, clippy, test)" } }

$Members = @(Get-Members)
if ($Crates.Count -gt 0) {
    $Selected = @($Crates | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ } |
        ForEach-Object { if ($_ -like 'bd_*') { $_ } else { "bd_$_" } })
    foreach ($c in $Selected) { if ($Members -notcontains $c) { throw "unknown crate '$c' (members: $($Members -join ', '))" } }
} else {
    $Selected = $Members
}

Write-Host "Brightest Day gate  |  crates: $($Selected -join ', ')  |  profile: $CargoProfile  |  target: $Target"
if ($PropScale -gt 0) { Write-Host "Property-test scale: x$PropScale" }

# ---------------------------------------------------------------------------------------- manifest

Write-Section 'manifest'
$WorkDir = $Root
$Broken = @()
$log = Join-Path $Logs 'manifest.log'
$code = Invoke-Logged 'cargo' @('fetch') $log -TimeoutSec 600
if ($code -eq 0) {
    Write-Status 'workspace' 'PASS' "$($Members.Count) members"
} else {
    $text = Get-Content $log -Raw
    $Broken = @([regex]::Matches($text, 'crates[\\/](bd_[a-z0-9_]+)[\\/]Cargo\.toml') | ForEach-Object { $_.Groups[1].Value } | Sort-Object -Unique)
    $Broken += @([regex]::Matches($text, 'could not parse `(bd_[a-z0-9_]+)`') | ForEach-Object { $_.Groups[1].Value })
    $Broken = @($Broken | Sort-Object -Unique)
    Write-Status 'workspace' 'FAIL' "manifest error in: $(if ($Broken) { $Broken -join ', ' } else { 'unknown (see log)' })"
    Show-Tail $log 6
}
# Declared targets whose source file is missing ([[example]]/[[bin]]/[[test]]/[[bench]]): cargo only
# notices when it builds that crate, and then refuses to build anything in the workspace.
foreach ($c in $Members) {
    $manifest = Join-Path $Root "crates\$c\Cargo.toml"
    if (-not (Test-Path $manifest)) { $Broken += $c; continue }
    $text = Get-Content $manifest -Raw
    foreach ($blk in [regex]::Matches($text, '(?ms)^\[\[(example|bin|test|bench)\]\](.*?)(?=^\[|\z)')) {
        $kind = $blk.Groups[1].Value
        $body = $blk.Groups[2].Value
        $name = [regex]::Match($body, '(?m)^\s*name\s*=\s*"([^"]+)"').Groups[1].Value
        $path = [regex]::Match($body, '(?m)^\s*path\s*=\s*"([^"]+)"').Groups[1].Value
        $dir = switch ($kind) { 'example' { 'examples' } 'test' { 'tests' } 'bench' { 'benches' } default { 'src/bin' } }
        $candidates = @(if ($path) { $path } else { "$dir\$name.rs"; "$dir\$name\main.rs" })
        if ($kind -eq 'bin' -and -not $path) { $candidates += 'src\main.rs' }
        $found = $candidates | Where-Object { Test-Path (Join-Path (Join-Path $Root "crates\$c") $_) }
        if (-not $found) {
            Write-Status "manifest:$c" 'FAIL' "declared $kind '$name' but $($candidates[0]) does not exist"
            $Broken += $c
        }
    }
}
$Broken = @($Broken | Sort-Object -Unique)
$Healthy = @($Selected | Where-Object { $Broken -notcontains $_ })
foreach ($b in @($Selected | Where-Object { $Broken -contains $_ })) {
    if (-not ($Results | Where-Object { $_.Stage -eq "manifest:$b" })) { Write-Status "manifest:$b" 'FAIL' 'crate manifest is broken; skipped' }
}
if ($Broken.Count -gt 0 -or $Isolated) {
    $blocked = @($Healthy | Where-Object { (Get-Closure @($_)) | Where-Object { $Broken -contains $_ } })
    foreach ($b in $blocked) { Write-Status "manifest:$b" 'FAIL' 'depends on a broken crate; skipped' }
    $Healthy = @($Healthy | Where-Object { $blocked -notcontains $_ })
    if ($Healthy.Count -gt 0) {
        $WorkDir = New-Mirror $Healthy
        # Separate target dir: cargo hashes path crates relative to the workspace root, so the mirror
        # and the real workspace would share (and wrongly reuse) each other's fingerprints.
        $env:CARGO_TARGET_DIR = Join-Path $Target 'ws-target'
        Write-Host "  (checking $($Healthy -join ', ') in isolated mirror $WorkDir)" -ForegroundColor DarkGray
    }
}

# --------------------------------------------------------------------------------------------- fmt

if ($Skip -notcontains 'fmt') {
    Write-Section 'fmt'
    $enforce = $StrictFmt -or ((Test-Path (Join-Path $Root 'rustfmt.toml')) -and -not $FmtReportOnly)
    if (-not $enforce) { Write-Host '  (report-only: no rustfmt.toml yet; use -StrictFmt to enforce)' -ForegroundColor DarkGray }
    foreach ($c in $Healthy) {
        $log = Join-Path $Logs "fmt-$c.log"
        $code = Invoke-Logged 'cargo' @('fmt', '-p', $c, '--check') $log -WorkDir $WorkDir -TimeoutSec 300
        $text = Get-Content $log
        $files = @($text | Select-String -Pattern '^Diff in (.+?)(:\d+| at line)' | ForEach-Object { $_.Matches[0].Groups[1].Value } | Sort-Object -Unique)
        $hunks = @($text | Select-String -Pattern '^Diff in ').Count
        $parseError = @($text | Select-String -Pattern '^error').Count -gt 0
        if ($code -eq 0) {
            Write-Status "fmt:$c" 'PASS'
        } elseif ($parseError -and $hunks -eq 0) {
            Write-Status "fmt:$c" 'FAIL' 'rustfmt could not parse the crate'
            Show-Tail $log 8
        } else {
            Write-Status "fmt:$c" $(if ($enforce) { 'FAIL' } else { 'WARN' }) "$hunks hunks in $($files.Count) files"
        }
    }
}

# ------------------------------------------------------------------------------------------ clippy

function Get-ClippyCounts([string]$Log, [string]$Crate) {
    $counts = @{}
    $seen = @{}
    foreach ($line in (Get-Content $Log)) {
        if (-not $line.StartsWith('{"reason":"compiler-message"')) { continue }
        $m = $line | ConvertFrom-Json
        if ($m.target.name -ne $Crate -and $m.package_id -notmatch "[/#]$Crate[@#]") { continue }
        $code = if ($m.message.code) { $m.message.code.code } else { '' }
        if (-not $code) { continue }
        $span = $m.message.spans | Select-Object -First 1
        $key = if ($span) { "$code|$($span.file_name)|$($span.line_start)|$($span.column_start)" } else { "$code|$($m.message.message)" }
        if ($seen.ContainsKey($key)) { continue }
        $seen[$key] = $true
        $counts[$code] = 1 + $(if ($counts.ContainsKey($code)) { $counts[$code] } else { 0 })
    }
    return $counts
}

$BudgetPath = Join-Path $PSScriptRoot 'lint_budget.json'
$Budget = @{}
if (Test-Path $BudgetPath) {
    $raw = Get-Content $BudgetPath -Raw | ConvertFrom-Json
    foreach ($p in $raw.PSObject.Properties) {
        if ($p.Name -like '_*') { continue }
        $h = @{}
        foreach ($q in $p.Value.PSObject.Properties) { $h[$q.Name] = [int]$q.Value }
        $Budget[$p.Name] = $h
    }
}
$NewBudget = @{}

if ($Skip -notcontains 'clippy') {
    Write-Section 'clippy'
    foreach ($c in $Healthy) {
        # Pass 1: every target (lib, bins, tests, examples) must compile and pass denied lints.
        $log = Join-Path $Logs "clippy-$c.log"
        $argv = @('clippy', '-p', $c, '--no-deps', '--all-targets', '--message-format=json', '--')
        if ($ClippyStrict) { $argv += @('-D', 'warnings') }
        $code = Invoke-Logged 'cargo' $argv $log -WorkDir $WorkDir
        $all = Get-ClippyCounts $log $c
        $errors = @(Get-Content $log | Where-Object { $_.StartsWith('{"reason":"compiler-message"') -and $_ -match '"level":"error"' }).Count
        $warnTotal = 0; foreach ($v in $all.Values) { $warnTotal += $v }
        if ($code -ne 0) {
            Write-Status "clippy:$c" 'FAIL' $(if ($errors -gt 0) { "$errors error(s)" } else { "cargo failed (exit $code)" })
            $msgs = Get-Content $log | Where-Object { $_ -match '"level":"error"' } | Select-Object -First 5 |
                ForEach-Object { ($_ | ConvertFrom-Json).message.rendered }
            foreach ($r in $msgs) { if ($r) { ($r -split "`n" | Select-Object -First 6) | ForEach-Object { Write-Host "         $_" -ForegroundColor DarkYellow } } }
            if (-not $msgs) { Show-Tail $log 10 }
            continue
        }
        # Pass 2: runtime code only (lib + bins, no tests) with the panicking-construct lints.
        $log2 = Join-Path $Logs "clippy-runtime-$c.log"
        $argv2 = @('clippy', '-p', $c, '--no-deps', '--message-format=json', '--')
        foreach ($l in $PanicLints) { $argv2 += @('-W', $l) }
        $null = Invoke-Logged 'cargo' $argv2 $log2 -WorkDir $WorkDir
        $rt = Get-ClippyCounts $log2 $c
        $panicCounts = [ordered]@{}
        foreach ($l in $PanicLints) { if ($rt.ContainsKey($l)) { $panicCounts[$l] = $rt[$l] } }
        $NewBudget[$c] = $panicCounts
        $summary = ($panicCounts.GetEnumerator() | ForEach-Object { "$($_.Key -replace 'clippy::', '')=$($_.Value)" }) -join ' '
        if (-not $summary) { $summary = 'none' }
        $detail = "$warnTotal warning(s); runtime panic-lints: $summary"
        $status = 'PASS'
        if ($Budget.ContainsKey($c) -and -not $UpdateLintBudget) {
            $over = @()
            foreach ($l in $panicCounts.Keys) {
                $allowed = if ($Budget[$c].ContainsKey($l)) { $Budget[$c][$l] } else { 0 }
                if ($panicCounts[$l] -gt $allowed) { $over += "$($l -replace 'clippy::', '') $($panicCounts[$l])>$allowed" }
            }
            if ($over) { $status = 'FAIL'; $detail += " | OVER BUDGET: $($over -join ', ')" }
        } elseif ($RuntimeCrates -contains $c -and $panicCounts.Count -gt 0) {
            $status = 'WARN'
        }
        Write-Status "clippy:$c" $status $detail
    }
    if ($UpdateLintBudget) {
        foreach ($k in $NewBudget.Keys) { $Budget[$k] = $NewBudget[$k] }
        $out = [ordered]@{ '_comment' = 'Max panicking-construct counts in runtime (non-test) code per crate. tools/check.ps1 fails if a crate exceeds its budget. Lower these as code is hardened; regenerate with -UpdateLintBudget.' }
        foreach ($k in ($Budget.Keys | Sort-Object)) { $out[$k] = $Budget[$k] }
        [System.IO.File]::WriteAllText($BudgetPath, ($out | ConvertTo-Json -Depth 4) + "`n", (New-Object System.Text.UTF8Encoding($false)))
        Write-Host "  lint budget written to $BudgetPath" -ForegroundColor DarkGray
    }
}

# -------------------------------------------------------------------------------------------- test

if ($Skip -notcontains 'test') {
    Write-Section "test ($CargoProfile profile)"
    $profileArgs = switch ($CargoProfile) { 'gate' { @('--profile', 'gate') } 'release' { @('--release') } default { @() } }
    foreach ($c in $Healthy) {
        $log = Join-Path $Logs "test-$c.log"
        $t0 = Get-Date
        $code = Invoke-Logged 'cargo' (@('test', '-p', $c, '--no-fail-fast') + $profileArgs) $log -WorkDir $WorkDir
        $secs = [int]((Get-Date) - $t0).TotalSeconds
        $text = Get-Content $log
        $passed = 0; $failed = 0
        foreach ($r in ($text | Select-String -Pattern 'test result: \w+\. (\d+) passed; (\d+) failed')) {
            $passed += [int]$r.Matches[0].Groups[1].Value; $failed += [int]$r.Matches[0].Groups[2].Value
        }
        if ($code -eq 0) {
            Write-Status "test:$c" 'PASS' "$passed passed (${secs}s)"
        } else {
            $why = if ($code -eq 124) { 'TIMEOUT' } elseif ($passed + $failed -eq 0) { 'build failed' } else { "$failed failed, $passed passed" }
            Write-Status "test:$c" 'FAIL' "$why (${secs}s)"
            Show-Tail $log 12 '^error(\[E\d+\])?:|panicked at|^test .* FAILED|minimal failing input|Test failed:|overflowed its stack|STATUS_'
        }
    }
}

# -------------------------------------------------------------------------------------------- soak

if ($Soak -gt 0) {
    Write-Section 'soak'
    $example = Join-Path $Root 'crates\bd_sim\examples\soak.rs'
    if (-not (Test-Path $example)) {
        Write-Status 'soak' 'SKIP' 'crates/bd_sim/examples/soak.rs does not exist yet'
    } elseif ($Broken -contains 'bd_sim') {
        Write-Status 'soak' 'FAIL' 'bd_sim manifest broken'
    } else {
        $log = Join-Path $Logs 'soak.log'
        # -Soak is in *simulated* seconds (120 ticks each). A fresh random seed every run explores new
        # worlds; the seed is printed so any failure replays exactly with -SoakSeed.
        $seed = if ($SoakSeed -ne 0) { $SoakSeed } else { [UInt64](Get-Random -Minimum 1 -Maximum 2147483647) }
        $ticks = [UInt64]$Soak * 120
        Write-Host "  soak: seed $seed, $ticks ticks" -ForegroundColor DarkGray
        $argv = @('run', '-p', 'bd_sim', '--profile', 'gate', '--example', 'soak', '--', '--seed', "$seed", '--ticks', "$ticks")
        $code = Invoke-Logged 'cargo' $argv $log -WorkDir $WorkDir
        if ($code -eq 0) {
            $summary = Get-Content $log | Select-String -Pattern '(?i)^(SOAK|PASS|ticks)' | Select-Object -Last 1
            Write-Status 'soak' 'PASS' $(if ($summary) { $summary.Line.Trim() } else { "$Soak s" })
        } else {
            Write-Status 'soak' 'FAIL' "exit $code; reproduce: cargo run -p bd_sim --profile gate --example soak -- --seed $seed --ticks $ticks"
            Show-Tail $log 15 'panicked at|invariant|FAIL|seed|error'
        }
    }
}

# ------------------------------------------------------------------------------- game: bench + smoke

function Build-Game {
    if ($script:GameExe) { return $script:GameExe }
    if ($Broken -contains 'bd_game') { return $null }
    $log = Join-Path $Logs 'build-game.log'
    Write-Host '  building brightest_day (release)...' -ForegroundColor DarkGray
    $code = Invoke-Logged 'cargo' @('build', '-p', 'bd_game', '--release', '--bin', 'brightest_day') $log -WorkDir $WorkDir
    if ($code -ne 0) {
        Show-Tail $log 12 '^error'
        return $null
    }
    $script:GameExe = Join-Path $env:CARGO_TARGET_DIR 'release\brightest_day.exe'
    return $script:GameExe
}
$script:GameExe = $null

function Read-Thresholds {
    $path = Join-Path $PSScriptRoot 'perf_thresholds.ron'
    $t = @{}
    if (Test-Path $path) {
        foreach ($m in [regex]::Matches((Get-Content $path -Raw), '(?m)^\s*([a-z0-9_]+)\s*:\s*([0-9.]+)')) { $t[$m.Groups[1].Value] = [double]$m.Groups[2].Value }
    }
    return $t
}

if ($Bench) {
    Write-Section 'bench'
    $exe = Build-Game
    if (-not $exe) {
        Write-Status 'bench' 'FAIL' 'brightest_day does not build (see logs\build-game.log)'
    } else {
        $json = Join-Path $Target 'bench.json'
        Remove-Item $json -ErrorAction SilentlyContinue
        $log = Join-Path $Logs 'bench.log'
        $code = Invoke-Logged $exe @('--bench', "$BenchSecs", '--bench-out', $json) $log -WorkDir $Root -TimeoutSec ($BenchSecs + 180)
        if ($code -ne 0 -or -not (Test-Path $json)) {
            Write-Status 'bench' 'FAIL' "exit $code, no $json (does this build support --bench?)"
            Show-Tail $log 10
        } else {
            $b = Get-Content $json -Raw | ConvertFrom-Json
            $th = Read-Thresholds
            $fails = @()
            $checks = @(@('frame_p99_ms', 'p99_ms'), @('frame_max_ms', 'max_ms'), @('frame_avg_ms', 'avg_ms'), @('hitches_16ms', 'hitches_16ms'))
            foreach ($pair in $checks) {
                $limit = $pair[0]; $field = $pair[1]
                if ($th.ContainsKey($limit) -and ($b.PSObject.Properties.Name -contains $field) -and [double]$b.$field -gt $th[$limit]) {
                    $fails += "$field $($b.$field) > $($th[$limit])"
                }
            }
            $detail = "p99 $($b.p99_ms) ms, max $($b.max_ms) ms, avg $($b.avg_ms) ms, hitches $($b.hitches_16ms)"
            if ($fails) { Write-Status 'bench' 'FAIL' "$detail | $($fails -join '; ')" } else { Write-Status 'bench' 'PASS' $detail }
        }
    }
}

function Test-Png([string]$Path) {
    Add-Type -AssemblyName System.Drawing
    $img = [System.Drawing.Bitmap]::FromFile($Path)
    try {
        $n = 0; $sum = 0.0; $sum2 = 0.0; $colors = @{}
        for ($y = 0; $y -lt 24; $y++) {
            for ($x = 0; $x -lt 32; $x++) {
                $px = $img.GetPixel([int](($x + 0.5) * $img.Width / 32), [int](($y + 0.5) * $img.Height / 24))
                $l = 0.2126 * $px.R + 0.7152 * $px.G + 0.0722 * $px.B
                $sum += $l; $sum2 += $l * $l; $n++
                $colors["$($px.R >> 3),$($px.G >> 3),$($px.B >> 3)"] = 1
            }
        }
        $mean = $sum / $n
        $std = [math]::Sqrt([math]::Max(0, $sum2 / $n - $mean * $mean))
        return [pscustomobject]@{ Width = $img.Width; Height = $img.Height; Mean = [math]::Round($mean, 1); Std = [math]::Round($std, 1); Colors = $colors.Count }
    } finally { $img.Dispose() }
}

if ($Smoke) {
    Write-Section 'smoke'
    $exe = Build-Game
    if (-not $exe) {
        Write-Status 'smoke' 'FAIL' 'brightest_day does not build (see logs\build-game.log)'
    } else {
        $png = Join-Path $Target 'smoke.png'
        Remove-Item $png -ErrorAction SilentlyContinue
        $log = Join-Path $Logs 'smoke.log'
        $code = Invoke-Logged $exe @('--screenshot', $png, '--quit-after', '8', '--windowed', '--size', '1280x720') $log -WorkDir $Root -TimeoutSec 240
        $panicked = @(Get-Content $log | Select-String -Pattern 'panicked at').Count -gt 0
        if (-not (Test-Path $png)) {
            Write-Status 'smoke' 'FAIL' "no screenshot (exit $code)"
            Show-Tail $log 12
        } else {
            $s = Test-Png $png
            $detail = "$($s.Width)x$($s.Height) mean $($s.Mean) std $($s.Std) colours $($s.Colors); exit $code"
            if ($panicked -or $code -ne 0) { Write-Status 'smoke' 'FAIL' "$detail (process failed)"; Show-Tail $log 8 'panicked at|error' }
            elseif ($s.Mean -lt 4 -or $s.Std -lt 2 -or $s.Colors -lt 8) { Write-Status 'smoke' 'FAIL' "$detail (blank/black image)" }
            else { Write-Status 'smoke' 'PASS' $detail }
        }
    }
}

# ------------------------------------------------------------------------------------------ summary

$elapsed = [int]((Get-Date) - $Started).TotalSeconds
$fail = @($Results | Where-Object Status -eq 'FAIL').Count
$warn = @($Results | Where-Object Status -eq 'WARN').Count
$pass = @($Results | Where-Object Status -eq 'PASS').Count
Write-Host ''
if ($fail -gt 0) {
    Write-Host "GATE: FAIL  ($fail failed, $warn warnings, $pass passed, ${elapsed}s)" -ForegroundColor Red
    $Results | Where-Object Status -eq 'FAIL' | ForEach-Object { Write-Host "  - $($_.Stage): $($_.Detail)" -ForegroundColor Red }
    exit 1
}
Write-Host "GATE: PASS  ($pass passed, $warn warnings, ${elapsed}s)" -ForegroundColor Green
exit 0
