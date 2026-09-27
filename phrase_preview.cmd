@echo off
rem PHRASE PREVIEW (dev tool, never shipped): type a phrase, see what the ring would make of it -
rem a plain-English readout plus a picture (target\phrase_preview\last.png, opened automatically).
rem Builds in release into its own target dir (target\tools, so it never fights the shared build;
rem the first build takes a few minutes, later ones are instant), then runs the interactive loop.
rem Flags pass through:  phrase_preview.cmd --spoken   phrase_preview.cmd --no-open "reinhardt charge"
setlocal
cd /d "%~dp0"
set "CARGO_TARGET_DIR=%~dp0target\tools"
echo Building the phrase preview tool...
cargo build --release -q -p bd_shapes --example phrase
if errorlevel 1 (
    echo.
    echo The build failed - see the errors above.
    pause
    exit /b 1
)
"%CARGO_TARGET_DIR%\release\examples\phrase.exe" %*
endlocal
