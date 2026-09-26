<#
.SYNOPSIS
  Generates test clips for bd_voice with Windows' built-in TTS (System.Speech).

.DESCRIPTION
  Must run under Windows PowerShell 5.1 (powershell.exe) - System.Speech is a .NET Framework
  assembly and is not available in pwsh 7. Writes 44.1 kHz 16-bit mono WAVs (a deliberately
  non-16 kHz rate so the resampler is exercised) to tools/voice/testwav/.

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File tools/voice/make_test_wavs.ps1
  powershell -NoProfile -ExecutionPolicy Bypass -File tools/voice/make_test_wavs.ps1 -Voice 'Microsoft Zira Desktop'
  (list voices: [System.Speech.Synthesis.SpeechSynthesizer]::new().GetInstalledVoices().VoiceInfo.Name)
#>
param(
    [string[]]$Phrases = @(
        'flaming hammer',
        'pikachu',
        'mjolnir',
        "captain america's shield",
        'bouncy wall with legs',
        'giant ice sword',
        'charizard'
    ),
    [int]$Rate = 0,   # SpeechSynthesizer speaking rate, -10..10
    [string]$Voice = ''   # e.g. 'Microsoft Zira Desktop'; default = system voice. Adds a suffix to file names.
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Speech

$Out = Join-Path $PSScriptRoot 'testwav'
New-Item -ItemType Directory -Force $Out | Out-Null

$fmt = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(
    44100, [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen, [System.Speech.AudioFormat.AudioChannel]::Mono)

foreach ($p in $Phrases) {
    $suffix = if ($Voice) { '_' + (($Voice -split ' ')[1]).ToLower() } else { '' }
    $file = Join-Path $Out (($p -replace "[^a-zA-Z0-9]+", '_').Trim('_') + $suffix + '.wav')
    $synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
    if ($Voice) { $synth.SelectVoice($Voice) }
    $synth.Rate = $Rate
    $synth.SetOutputToWaveFile($file, $fmt)
    $synth.Speak($p)
    $synth.Dispose()
    Write-Host "wrote $file"
}
