@echo off
set PATH=C:\msys64\mingw64\bin;%USERPROFILE%\.cargo\bin;%PATH%
set COEVO_DATABASE_URL=sqlite:C:/coevo-build/coevo/data/coevo.db?mode=rwc
cd /d C:\coevo-build\coevo
cargo run -p coevo-server > C:\coevo-build\coevo\server.log 2>&1
