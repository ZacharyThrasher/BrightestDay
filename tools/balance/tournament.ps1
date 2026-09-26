# Balance tournament (COMBAT.md §5i.6): N headless 8-bot FFA matches → target/balance/report.json
# + docs/BALANCE_REPORT.md. Extra arguments are passed through to the example, e.g.
#   tools/balance/tournament.ps1 -Matches 500
#   tools/balance/tournament.ps1 -Matches 500 -After assets/tuning/combat.ron   # before/after
param(
    [int]$Matches = 500,
    [int]$Secs = 240,
    [string]$Before = "default",
    [string]$After = "",
    [string]$Md = "docs/BALANCE_REPORT.md",
    [string]$Json = "target/balance/report.json"
)
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "../..")
Push-Location $root
try {
    $env:CARGO_TARGET_DIR = Join-Path $root "target/balance"
    $a = @("run", "-p", "bd_sim", "--release", "--example", "tournament", "--", "--matches", $Matches, "--secs", $Secs, "--before", $Before, "--md", $Md, "--json", $Json)
    if ($After) { $a += @("--after", $After) }
    & cargo @a
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
