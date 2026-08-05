@echo off
setlocal
cd /d "%~dp0"
title Adelia Development Server

if not exist ".env" (
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\setup-windows.ps1"
    if errorlevel 1 (
        echo.
        echo Adelia setup failed. Read the error above.
        exit /b 1
    )
)

cargo run -- serve
