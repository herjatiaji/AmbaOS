#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ISO_PATH="$ROOT_DIR/ambaos.iso"
STATE_IMG="$ROOT_DIR/ambaos_state.img"

if [ ! -f "$ISO_PATH" ]; then
    "$ROOT_DIR/scripts/build.sh"
fi

if [ ! -f "$STATE_IMG" ]; then
    truncate -s 4M "$STATE_IMG"
fi

GW_PID=""
cleanup() {
    if [ -n "$GW_PID" ]; then
        kill "$GW_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

if ! python3 -c "import socket; s=socket.socket(); s.settimeout(0.3); s.connect(('127.0.0.1', 18088)); s.close()" 2>/dev/null; then
    python3 "$ROOT_DIR/scripts/amba_web_gateway.py" >/dev/null 2>&1 &
    GW_PID=$!
    sleep 0.2
fi

AUDIO_FLAGS=()
if [ -S "/mnt/wslg/PulseServer" ]; then
    export PULSE_SERVER="${PULSE_SERVER:-unix:/mnt/wslg/PulseServer}"
    AUDIO_FLAGS=(-audiodev pa,id=snd0 -machine pcspk-audiodev=snd0)
elif [ -n "${PULSE_SERVER:-}" ]; then
    AUDIO_FLAGS=(-audiodev pa,id=snd0 -machine pcspk-audiodev=snd0)
fi

if [ "${1:-}" = "--uefi" ]; then
    OVMF_CODE="/usr/share/OVMF/OVMF_CODE_4M.fd"
    [ -f "$OVMF_CODE" ] || OVMF_CODE="/usr/share/OVMF/OVMF_CODE.fd"
    qemu-system-x86_64 \
        -M q35,vmport=on \
        -m 2G \
        -display gtk,show-cursor=off \
        -device piix3-ide,id=ide0 \
        -drive file="$STATE_IMG",format=raw,if=none,id=disk0 \
        -device ide-hd,drive=disk0,bus=ide0.0 \
        -nic user,model=e1000 \
        "${AUDIO_FLAGS[@]}" \
        -drive if=pflash,unit=0,format=raw,file="$OVMF_CODE",readonly=on \
        -cdrom "$ISO_PATH" \
        -boot d \
        -serial stdio
else
    qemu-system-x86_64 \
        -M q35,vmport=on \
        -m 2G \
        -display gtk,show-cursor=off \
        -device piix3-ide,id=ide0 \
        -drive file="$STATE_IMG",format=raw,if=none,id=disk0 \
        -device ide-hd,drive=disk0,bus=ide0.0 \
        -nic user,model=e1000 \
        "${AUDIO_FLAGS[@]}" \
        -cdrom "$ISO_PATH" \
        -boot d \
        -serial stdio
fi
