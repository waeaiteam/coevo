@echo off
set PATH=C:\Users\wae\.cargo\bin;%PATH%
cd /d %~dp0
call npm run tauri dev
