@echo off
setlocal
title Adelia PostgreSQL Setup
cd /d "%~dp0"

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\setup-windows.ps1"
set "ADELIA_SETUP_EXIT=%ERRORLEVEL%"

echo.
if not "%ADELIA_SETUP_EXIT%"=="0" (
    echo Database setup did not complete. Read the error above.
) else (
    echo Database setup completed successfully.
)
echo.
pause
exit /b %ADELIA_SETUP_EXIT%
