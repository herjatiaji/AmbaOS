@echo off
title AmbaOS v0.5 - QEMU Launcher
wsl -d Ubuntu -- bash -lc "cd '/mnt/e/digifolks-and-personal-projects/oprekpixel/os' && sed -i 's/\r$//' scripts/*.sh scripts/*.py && chmod +x scripts/*.sh && ./scripts/run_qemu.sh"
pause
