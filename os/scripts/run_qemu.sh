#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ISO_PATH="$ROOT_DIR/ambaos.iso"
STATE_IMG="$ROOT_DIR/ambaos_state.img"

USE_UEFI=0
USE_VNC=0

for arg in "$@"; do
    case "$arg" in
        --uefi) USE_UEFI=1 ;;
        --vnc)  USE_VNC=1 ;;
        -h|--help)
            echo "Usage: ./scripts/run_qemu.sh [--uefi] [--vnc]"
            echo "  --uefi   Boot using OVMF UEFI firmware"
            echo "  --vnc    Use VNC display on 127.0.0.1:5900 (auto-enabled if no GUI display)"
            exit 0
            ;;
        *)
            echo "[WARN] Unknown option ignored: $arg" >&2
            ;;
    esac
done

if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
    echo "[ERROR] qemu-system-x86_64 not found. Install it with:" >&2
    echo "  sudo apt update && sudo apt install -y qemu-system-x86 python3 python3-pil" >&2
    exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
    echo "[ERROR] python3 not found. Install it with: sudo apt install -y python3 python3-pil" >&2
    exit 1
fi

if [ ! -f "$ISO_PATH" ]; then
    echo "[INFO] ambaos.iso not found, running build.sh..."
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

ACCEL_FLAGS=(-accel tcg,thread=multi,tb-size=512 -smp 4)
if [ -r /dev/kvm ] && [ -w /dev/kvm ]; then
    ACCEL_FLAGS=(-enable-kvm -cpu host -smp 4)
else
    echo "[INFO] /dev/kvm not writable (using software TCG). To enable hardware speed:"
    echo "       sudo usermod -aG kvm \$USER && sudo chmod 666 /dev/kvm"
fi

RUN_ISO="/tmp/ambaos_run.iso"
cp -f "$ISO_PATH" "$RUN_ISO"

DISPLAY_FLAGS=(-display gtk,show-cursor=off)
if [ "$USE_VNC" -eq 1 ] || ( [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ] && [ ! -d "/mnt/wslg" ] ); then
    DISPLAY_FLAGS=(-vnc 127.0.0.1:0)
    echo "[INFO] No GUI display detected or --vnc requested. VNC server listening at 127.0.0.1:5900"
fi

AUDIO_FLAGS=()
if [ -S "/mnt/wslg/PulseServer" ]; then
    export PULSE_SERVER="${PULSE_SERVER:-unix:/mnt/wslg/PulseServer}"
    AUDIO_FLAGS=(-audiodev pa,id=snd0 -machine pcspk-audiodev=snd0)
elif [ -n "${PULSE_SERVER:-}" ]; then
    AUDIO_FLAGS=(-audiodev pa,id=snd0 -machine pcspk-audiodev=snd0)
fi

QEMU_ARGS=(
    -M q35,vmport=on
    "${ACCEL_FLAGS[@]}"
    -m 2G
    "${DISPLAY_FLAGS[@]}"
    -device piix3-ide,id=ide0
    -drive "file=$STATE_IMG,format=raw,if=none,id=disk0"
    -device ide-hd,drive=disk0,bus=ide0.0
    -nic user,model=e1000
    "${AUDIO_FLAGS[@]}"
    -cdrom "$RUN_ISO"
    -boot d
    -serial stdio
)

if [ "$USE_UEFI" -eq 1 ]; then
    OVMF_CODE="/usr/share/OVMF/OVMF_CODE_4M.fd"
    [ -f "$OVMF_CODE" ] || OVMF_CODE="/usr/share/OVMF/OVMF_CODE.fd"
    if [ ! -f "$OVMF_CODE" ]; then
        echo "[ERROR] OVMF firmware not found. Install with: sudo apt install -y ovmf" >&2
        exit 1
    fi
    QEMU_ARGS+=(-drive "if=pflash,unit=0,format=raw,file=$OVMF_CODE,readonly=on")
fi

qemu-system-x86_64 "${QEMU_ARGS[@]}"
