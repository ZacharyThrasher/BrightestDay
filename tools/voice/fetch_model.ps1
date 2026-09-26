<#
.SYNOPSIS
  Downloads the Whisper speech model(s) used by bd_voice into assets/models/.

.DESCRIPTION
  Models come from the official whisper.cpp Hugging Face repo (ggerganov/whisper.cpp),
  pinned to a specific repo commit, and are verified against their SHA256 (the Git LFS oid).
  Files that already exist with the right hash are skipped.

  Default: ggml-base.en-q5_1.bin (57 MiB, the model bd_voice loads by default).
  -Full also fetches ggml-base.en.bin (141 MiB, f16, marginally more accurate, ~same speed).
  -Tiny also fetches ggml-tiny.en-q5_1.bin (31 MiB, faster, noticeably less accurate).

.EXAMPLE
  pwsh tools/voice/fetch_model.ps1
  pwsh tools/voice/fetch_model.ps1 -Full
#>
param(
    [switch]$Full,
    [switch]$Tiny
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'   # Invoke-WebRequest is ~10x faster without the progress bar

# Pinned commit of https://huggingface.co/ggerganov/whisper.cpp
$Commit = '5359861c739e955e79d9a303bcbc70fb988958b1'
$Base   = "https://huggingface.co/ggerganov/whisper.cpp/resolve/$Commit"

$Models = [ordered]@{
    'ggml-base.en-q5_1.bin' = @{ Sha256 = '4baf70dd0d7c4247ba2b81fafd9c01005ac77c2f9ef064e00dcf195d0e2fdd2f'; Size = 59721011 }
}
if ($Full) {
    $Models['ggml-base.en.bin'] = @{ Sha256 = 'a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002'; Size = 147964211 }
}
if ($Tiny) {
    $Models['ggml-tiny.en-q5_1.bin'] = @{ Sha256 = 'c77c5766f1cef09b6b7d47f21b546cbddd4157886b3b5d6d4f709e91e66c7c2b'; Size = 32166155 }
}

$Root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$Dest = Join-Path $Root 'assets\models'
New-Item -ItemType Directory -Force $Dest | Out-Null

# Keep the (large) model files out of git.
$Ignore = Join-Path $Dest '.gitignore'
if (-not (Test-Path $Ignore)) {
    Set-Content -Path $Ignore -Value "*`n!.gitignore`n" -NoNewline
}

foreach ($name in $Models.Keys) {
    $want = $Models[$name].Sha256
    $path = Join-Path $Dest $name
    if (Test-Path $path) {
        $have = (Get-FileHash -Algorithm SHA256 $path).Hash.ToLowerInvariant()
        if ($have -eq $want) { Write-Host "ok        $name (already present)"; continue }
        Write-Host "mismatch  $name (have $have) - re-downloading"
    }
    $url = "$Base/$name"
    $tmp = "$path.part"
    Write-Host ("fetching  {0} ({1:N1} MiB) from {2}" -f $name, ($Models[$name].Size / 1MB), $url)
    Invoke-WebRequest -Uri $url -OutFile $tmp -UseBasicParsing
    $have = (Get-FileHash -Algorithm SHA256 $tmp).Hash.ToLowerInvariant()
    if ($have -ne $want) {
        Remove-Item $tmp -Force
        throw "SHA256 mismatch for ${name}: expected $want, got $have"
    }
    Move-Item -Force $tmp $path
    Write-Host "ok        $name"
}
