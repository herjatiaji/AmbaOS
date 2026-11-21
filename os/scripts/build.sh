#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LIMINE_DIR="$HOME/.cache/irengos-limine"
[ -x "$LIMINE_DIR/limine" ] || LIMINE_DIR="$HOME/.cache/ambaos-limine"
TARGET_DIR="$HOME/.cache/ambaos-target"
ISO_ROOT="/tmp/ambaos_iso_root"

export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="$TARGET_DIR"

if [ ! -x "$LIMINE_DIR/limine" ]; then
    echo "Limine not found at $LIMINE_DIR. Run setup first." >&2
    exit 1
fi

echo "[1/5] Generating AmbaOS logo sprites & anti-aliased fonts..."
python3 "$ROOT_DIR/scripts/gen_assets.py"

echo "[2/5] Building initramfs.tar..."
mkdir -p "$ROOT_DIR/build"
tar --format=ustar -cf "$ROOT_DIR/build/initramfs.tar" -C "$ROOT_DIR/initramfs" .

echo "[3/5] Building ambaos-kernel (x86_64-unknown-none release)..."
(cd "$ROOT_DIR/kernel" && cargo build --release)
KERNEL_BIN="$TARGET_DIR/x86_64-unknown-none/release/ambaos-kernel"

echo "[4/5] Staging ISO filesystem..."
rm -rf "$ISO_ROOT"
mkdir -p "$ISO_ROOT/boot/limine" "$ISO_ROOT/EFI/BOOT"
cp "$KERNEL_BIN" "$ISO_ROOT/boot/kernel"
cp "$ROOT_DIR/build/initramfs.tar" "$ISO_ROOT/boot/initramfs.tar"
cp "$ROOT_DIR/boot/limine.conf" "$ISO_ROOT/boot/limine/limine.conf"
cp "$LIMINE_DIR/limine-bios.sys" "$LIMINE_DIR/limine-bios-cd.bin" "$LIMINE_DIR/limine-uefi-cd.bin" "$ISO_ROOT/boot/limine/"
cp "$LIMINE_DIR/BOOTX64.EFI" "$ISO_ROOT/EFI/BOOT/"

echo "[5/5] Creating ambaos.iso..."
xorriso -as mkisofs \
    -b boot/limine/limine-bios-cd.bin \
    -no-emul-boot -boot-load-size 4 -boot-info-table \
    --efi-boot boot/limine/limine-uefi-cd.bin \
    -efi-boot-part --efi-boot-image --protective-msdos-label \
    "$ISO_ROOT" -o "$ROOT_DIR/ambaos.iso" >/dev/null 2>&1

"$LIMINE_DIR/limine" bios-install "$ROOT_DIR/ambaos.iso"
rm -rf "$ISO_ROOT"

ls -lh "$ROOT_DIR/ambaos.iso"
echo "BUILD_OK"
