#!/usr/bin/env python3
"""Generate de-matted RGBA logo sprites and anti-aliased TrueType font atlases for AmbaOS."""

import os
import struct
from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LOGO_SRC = os.path.join(ROOT, "assets", "logo.jpeg")
ASSETS_DIR = os.path.join(ROOT, "kernel", "src", "gui", "assets")

FONT_SPECS = [
    ("font_ui.bin", "/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf", 14),
    ("font_ui_bold.bin", "/usr/share/fonts/truetype/ubuntu/Ubuntu-B.ttf", 14),
    ("font_mono.bin", "/usr/share/fonts/truetype/ubuntu/UbuntuMono-R.ttf", 16),
]

LOGO_SIZES = [120, 48, 22]


def generate_logos() -> None:
    img = Image.open(LOGO_SRC).convert("RGB")
    w, h = img.size
    pixels = img.load()

    # Tight bounding box around non-white foreground
    min_x, min_y = w, h
    max_x, max_y = 0, 0
    for y in range(h):
        for x in range(w):
            r, g, b = pixels[x, y]
            if min(r, g, b) < 238:
                if x < min_x:
                    min_x = x
                if y < min_y:
                    min_y = y
                if x > max_x:
                    max_x = x
                if y > max_y:
                    max_y = y

    bw = max_x - min_x + 1
    bh = max_y - min_y + 1
    side = int(max(bw, bh) * 1.08)

    cropped = img.crop((min_x, min_y, max_x + 1, max_y + 1))
    square = Image.new("RGB", (side, side), (255, 255, 255))
    square.paste(cropped, ((side - bw) // 2, (side - bh) // 2))

    for size in LOGO_SIZES:
        resized = square.resize((size, size), Image.Resampling.LANCZOS)
        src_px = resized.load()
        out = bytearray(size * size * 4)
        idx = 0
        for y in range(size):
            for x in range(size):
                r, g, b = src_px[x, y]
                lum_min = min(r, g, b)
                if lum_min >= 246:
                    a = 0
                    fg_r = fg_g = fg_b = 0
                else:
                    # Extract smooth opacity against white background and de-matte RGB
                    a_f = min(1.0, (255.0 - lum_min) / 245.0)
                    a = max(0, min(255, int(round(a_f * 255.0))))
                    inv_bg = 255.0 * (1.0 - a_f)
                    fg_r = max(0, min(255, int(round((r - inv_bg) / a_f))))
                    fg_g = max(0, min(255, int(round((g - inv_bg) / a_f))))
                    fg_b = max(0, min(255, int(round((b - inv_bg) / a_f))))

                out[idx] = fg_r
                out[idx + 1] = fg_g
                out[idx + 2] = fg_b
                out[idx + 3] = a
                idx += 4

        out_path = os.path.join(ASSETS_DIR, f"logo_{size}.rgba")
        with open(out_path, "wb") as f:
            f.write(out)
        print(f"Generated {os.path.relpath(out_path, ROOT)} ({len(out)} bytes)")


def generate_font(out_name: str, ttf_path: str, size: int) -> None:
    font = ImageFont.truetype(ttf_path, size)
    ascent, descent = font.getmetrics()
    line_height = min(255, ascent + descent + 1)

    glyph_meta = bytearray()
    glyph_pixels = bytearray()

    for code in range(32, 127):
        ch = chr(code)
        adv = max(1, min(255, int(round(font.getlength(ch)))))

        canvas = Image.new("L", (48, line_height + 6), 0)
        draw = ImageDraw.Draw(canvas)
        draw.text((0, ascent), ch, fill=255, font=font, anchor="ls")
        bbox = canvas.getbbox()

        if bbox is None or ch == " ":
            glyph_meta.extend(struct.pack("<IBBBb", len(glyph_pixels), 0, 0, adv, 0))
            continue

        x0, y0, x1, y1 = bbox
        # Keep x starting at 0 so bearing aligns naturally with advance
        crop_w = min(255, max(1, x1))
        crop_h = min(255, max(1, y1 - y0))
        y_off = max(-128, min(127, y0))

        cropped = canvas.crop((0, y0, crop_w, y0 + crop_h))
        offset = len(glyph_pixels)
        glyph_pixels.extend(cropped.tobytes())
        glyph_meta.extend(struct.pack("<IBBBb", offset, crop_w, crop_h, adv, y_off))

    header = struct.pack("<BBH", line_height, min(255, ascent), 95)
    out_path = os.path.join(ASSETS_DIR, out_name)
    with open(out_path, "wb") as f:
        f.write(header)
        f.write(glyph_meta)
        f.write(glyph_pixels)
    print(f"Generated {os.path.relpath(out_path, ROOT)} (line_h={line_height}, {4 + len(glyph_meta) + len(glyph_pixels)} bytes)")


def main() -> None:
    os.makedirs(ASSETS_DIR, exist_ok=True)
    generate_logos()
    for out_name, ttf_path, size in FONT_SPECS:
        generate_font(out_name, ttf_path, size)


if __name__ == "__main__":
    main()
