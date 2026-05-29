@echo off
REM Build coevo-server sidecar — wrapper calling the Node.js script
echo Building coevo-server sidecar via Node.js script...
node "%~dp0scripts\build-sidecar.mjs"
if %ERRORLEVEL% neq 0 (
    echo FAILED. See output above.
    exit /b %ERRORLEVEL%
)
echo Done.
