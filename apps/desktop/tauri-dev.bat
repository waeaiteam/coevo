@echo off
set PATH=C:\Users\wae\.cargo\bin;C:\msys64\mingw64\bin;%PATH%
cd /d "D:\多智能体平台coevoV1正式发布版\coevo\apps\desktop"
call npm run tauri dev
