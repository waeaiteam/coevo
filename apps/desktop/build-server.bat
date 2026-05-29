@echo off
REM Build coevo-server and copy to Tauri sidecar directory
echo Building coevo-server sidecar...

cd /d "%~dp0..\..\..\.."
set CARGO=cargo
where cargo >nul 2>&1 || set CARGO=%USERPROFILE%\.cargo\bin\cargo.exe

REM Build release
echo Running: %CARGO% build --release -p coevo-server
%CARGO% build --release -p coevo-server
if %ERRORLEVEL% neq 0 (
    echo FAILED: cargo build --release -p coevo-server
    exit /b 1
)

REM Copy to Tauri sidecar directory
set SIDECAR_DIR=apps\desktop\src-tauri\binaries
if not exist "%SIDECAR_DIR%" mkdir "%SIDECAR_DIR%"

REM Determine target triple
for /f "tokens=*" %%i in ('%CARGO% -vV ^| findstr "host:"') do set TRIPLE=%%i
set TRIPLE=%TRIPLE:~6%

REM Copy with target triple suffix for Tauri v2 externalBin
copy /Y "target\release\coevo-server.exe" "%SIDECAR_DIR%\coevo-server-%TRIPLE%.exe"
if %ERRORLEVEL% neq 0 (
    echo WARNING: Could not copy to %SIDECAR_DIR%\coevo-server-%TRIPLE%.exe
    echo Trying without triple suffix...
    copy /Y "target\release\coevo-server.exe" "%SIDECAR_DIR%\coevo-server.exe"
)

echo coevo-server sidecar built and copied.
