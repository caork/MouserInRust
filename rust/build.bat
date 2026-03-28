@echo off
REM Build Mouser Rust binary for Windows.
setlocal

set "SCRIPT_DIR=%~dp0"
cd /d "%SCRIPT_DIR%"

echo Building Mouser (Rust) for Windows...
cargo build --release
if errorlevel 1 (
    echo Build failed.
    exit /b 1
)

set "BINARY=target\release\mouser.exe"
set "DIST=dist-rust"
if not exist "%DIST%" mkdir "%DIST%"
copy /Y "%BINARY%" "%DIST%\mouser.exe" >nul

echo.
echo Build complete: %DIST%\mouser.exe
for %%F in ("%DIST%\mouser.exe") do echo Binary size: %%~zF bytes
