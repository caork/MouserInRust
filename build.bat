@echo off
:: ──────────────────────────────────────────────────────────────
:: build.bat — Build a portable LogiControl distribution
::
:: Produces:  dist\LogiControl\LogiControl.exe   (+ supporting files)
:: Zip that folder and distribute — no Python install required.
:: ──────────────────────────────────────────────────────────────
title LogiControl — Build
cd /d "%~dp0"

echo.
echo ===  LogiControl Portable Build  ===
echo.

:: ── 1. Activate venv if present ──────────────────────────────
if exist ".venv\Scripts\activate.bat" (
    call ".venv\Scripts\activate.bat"
    echo [*] Virtual-env activated
) else (
    echo [!] No .venv found — using system Python
)

:: ── 2. Ensure PyInstaller is installed ───────────────────────
pip show pyinstaller >nul 2>&1
if %errorlevel% neq 0 (
    echo [*] Installing PyInstaller...
    pip install pyinstaller
)

:: ── 3. Clean previous build ──────────────────────────────────
if exist "dist\LogiControl" (
    echo [*] Removing previous dist\LogiControl...
    rmdir /s /q "dist\LogiControl"
)
if exist "build\LogiControl" (
    rmdir /s /q "build\LogiControl"
)

:: ── 4. Run PyInstaller ───────────────────────────────────────
echo [*] Building with PyInstaller...
pyinstaller LogiControl.spec --noconfirm

if %errorlevel% neq 0 (
    echo.
    echo [ERROR] Build failed — see messages above.
    pause
    exit /b 1
)

:: ── 5. Copy default config if missing ────────────────────────
:: (not needed — config is auto-created at first run in %APPDATA%)

echo.
echo ===  Build complete!  ===
echo Output: dist\LogiControl\LogiControl.exe
echo.
echo To distribute: zip the  dist\LogiControl  folder.
echo.
pause
