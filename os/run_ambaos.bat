@echo off
setlocal enabledelayedexpansion
title AmbaOS v0.7.1 - Universal QEMU Launcher

where wsl >nul 2>&1
if errorlevel 1 (
    echo [ERROR] WSL2 is not installed or not on PATH.
    echo Install WSL2 with: wsl --install -d Ubuntu
    pause
    exit /b 1
)

set "WIN_OS_DIR=%~dp0"
if "%WIN_OS_DIR:~-1%"=="\" set "WIN_OS_DIR=%WIN_OS_DIR:~0,-1%"

echo Launching AmbaOS from: %WIN_OS_DIR%
wsl bash -lc "OS_DIR=\"$(wslpath -a '%WIN_OS_DIR%')\" && cd \"$OS_DIR\" && sed -i 's/\r$//' scripts/*.sh scripts/*.py && chmod +x scripts/*.sh && ./scripts/run_qemu.sh %*"
if errorlevel 1 (
    echo.
    echo [ERROR] Failed to launch AmbaOS. Make sure qemu-system-x86_64 and python3 are installed in WSL.
    pause
)
