<#
.SYNOPSIS
  VOICE BENCH driver (dev tool, never shipped): corpus -> TTS -> Whisper -> toolbox reader -> report.

.DESCRIPTION
  See tools/voice_bench/README.md. Everything generated lands in target\voice_bench\ (ignored).
  Stages re-use what exists: the corpus (-Corpus rebuilds it), the TTS audio (a cache), the
  transcripts per -Tag (-Transcribe re-runs Whisper).

.EXAMPLE
  pwsh -File tools\voice_bench\run.ps1 -Corpus -Tag new -Prompt new -Stream
  pwsh -File tools\voice_bench\run.ps1 -Tag before -Prompt old -Norm old -Raw     # the pre-fluency path
#>
param(
    [string]$Tag = 'new',
    [ValidateSet('old', 'new')][string]$Prompt = 'new',
    [ValidateSet('old', 'new')][string]$Norm = 'new',
    [switch]$Raw,          # score the clean transcript as-is (no voice_fluency::heard_line)
    [switch]$Stream,       # cut clips at pauses and transcribe beat by beat (the streaming path)
    [switch]$Corpus,       # rebuild the corpus / TTS job list / prompts
    [switch]$Transcribe,   # re-run Whisper even if heard_<Tag>.tsv exists
    [int]$Limit = 0
)
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$out = Join-Path $root 'target\voice_bench'
New-Item -ItemType Directory -Force $out | Out-Null
$env:CARGO_BUILD_JOBS = '8'
$env:BD_HIDDEN_WINDOW = '1'

function Test-Bin([string]$name) {
    # the reader stages are ignored tests in bd_game's test binary (shared target dir)
    $env:CARGO_TARGET_DIR = Join-Path $root 'target\play'
    cargo test -p bd_game --bin brightest_day $name -- --ignored --nocapture
    if ($LASTEXITCODE -ne 0) { throw "$name failed" }
}

Push-Location $root
try {
    if ($Corpus -or -not (Test-Path (Join-Path $out 'corpus.tsv'))) {
        Write-Host '== corpus'
        Test-Bin 'voice_bench_corpus'
    }
    Write-Host '== tts (Windows PowerShell 5.1: System.Speech + OneCore voices)'
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'tts.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'tts failed' }

    $heard = Join-Path $out "heard_$Tag.tsv"
    if ($Transcribe -or -not (Test-Path $heard)) {
        Write-Host '== whisper (bd_voice, the game engine, own target dir)'
        $env:CARGO_TARGET_DIR = Join-Path $root 'target\voice_fluid'
        cargo build -p bd_voice --features whisper --profile fastrel --example voice_bench
        if ($LASTEXITCODE -ne 0) { throw 'voice_bench build failed' }
        $a = @('--prompt-file', (Join-Path $out "prompt_$Prompt.txt"), '--out', $heard)
        if ($Stream) { $a += '--stream' }
        if ($Limit -gt 0) { $a += @('--limit', "$Limit") }
        & (Join-Path $root 'target\voice_fluid\fastrel\examples\voice_bench.exe') @a
        if ($LASTEXITCODE -ne 0) { throw 'transcription failed' }
    }

    Write-Host '== score'
    $env:BENCH_HEARD = "heard_$Tag.tsv"
    $env:BENCH_TAG = $Tag
    $env:BENCH_RAW = if ($Raw) { '1' } else { '0' }
    $env:BENCH_NORM = $Norm
    Test-Bin 'voice_bench_score'
    Write-Host "report: $out\report_$Tag.md"
} finally {
    Pop-Location
}
