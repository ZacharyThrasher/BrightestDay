<#
.SYNOPSIS
  VOICE BENCH stage 1: speaks every line of the bench corpus with Windows' built-in TTS.

.DESCRIPTION
  Dev tool, never shipped. Must run under Windows PowerShell 5.1 (powershell.exe): System.Speech is a
  .NET Framework assembly, and the WinRT (OneCore) voices are reached through the 5.1 WinRT bridge.

  Input: a TSV (target\voice_bench\tts_jobs.tsv, written by the corpus stage) with lines
      <wav file name> <TAB> <voice> <TAB> <ssml body>
  voice = sapi:David | sapi:Zira (desktop SAPI 5 engines) | rt:David | rt:Zira | rt:Mark (OneCore).
  The SSML body is wrapped in <speak>; it may carry <prosody>, <break> and <emphasis>.
  Existing WAVs are kept (the audio is a cache: delete target\voice_bench\wav to re-speak).

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File tools\voice_bench\tts.ps1
#>
param(
    [string]$Jobs = (Join-Path $PSScriptRoot '..\..\target\voice_bench\tts_jobs.tsv'),
    [string]$Out = (Join-Path $PSScriptRoot '..\..\target\voice_bench\wav')
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Speech
Add-Type -AssemblyName System.Runtime.WindowsRuntime
[void][Windows.Media.SpeechSynthesis.SpeechSynthesizer, Windows.Media.SpeechSynthesis, ContentType = WindowsRuntime]
[void][Windows.Storage.Streams.DataReader, Windows.Storage.Streams, ContentType = WindowsRuntime]

New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path

# WinRT async -> Task (PowerShell 5.1 cannot await IAsyncOperation directly).
$asTask = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
        $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'
    })[0]
function Await($op, [Type]$t) {
    $task = $asTask.MakeGenericMethod($t).Invoke($null, @($op))
    [void]$task.Wait(-1)
    $task.Result
}

$fmt = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(
    22050, [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen, [System.Speech.AudioFormat.AudioChannel]::Mono)
$sapi = New-Object System.Speech.Synthesis.SpeechSynthesizer
$rt = New-Object Windows.Media.SpeechSynthesis.SpeechSynthesizer
$rtVoices = @{}
foreach ($v in [Windows.Media.SpeechSynthesis.SpeechSynthesizer]::AllVoices) { $rtVoices[($v.DisplayName -split ' ')[-1]] = $v }

$n = 0; $made = 0
foreach ($line in [System.IO.File]::ReadAllLines((Resolve-Path $Jobs).Path)) {
    if (-not $line -or $line.StartsWith('#')) { continue }
    $f = $line.Split("`t")
    if ($f.Count -lt 3) { continue }
    $n++
    $file = Join-Path $Out $f[0]
    if (Test-Path $file) { continue }
    $engine, $voice = $f[1].Split(':')
    $ssml = "<speak version='1.0' xmlns='http://www.w3.org/2001/10/synthesis' xml:lang='en-US'>" + $f[2] + "</speak>"
    if ($engine -eq 'sapi') {
        $sapi.SelectVoice("Microsoft $voice Desktop")
        $sapi.SetOutputToWaveFile($file, $fmt)
        $sapi.SpeakSsml($ssml)
        $sapi.SetOutputToNull()
    } else {
        $rt.Voice = $rtVoices[$voice]
        $stream = Await ($rt.SynthesizeSsmlToStreamAsync($ssml)) ([Windows.Media.SpeechSynthesis.SpeechSynthesisStream])
        $size = [uint32]$stream.Size
        $reader = New-Object Windows.Storage.Streams.DataReader($stream.GetInputStreamAt(0))
        [void](Await ($reader.LoadAsync($size)) ([uint32]))
        $bytes = New-Object byte[] $size
        $reader.ReadBytes($bytes)
        [System.IO.File]::WriteAllBytes($file, $bytes)
        $reader.Dispose(); $stream.Dispose()
    }
    $made++
    if ($made % 100 -eq 0) { Write-Host "  tts: $made new clips ($n jobs read)" }
}
$sapi.Dispose()
Write-Host "tts: $n jobs, $made new clips -> $Out"
