"""Draws MIDI Spyglass's app icon (icon/AppIcon.icns, icon/icon-1024.png, and the Android launcher icons):
the brass spyglass with a note on a staff. Needs Pillow; macOS's iconutil makes the .icns.
    python3 tools/make_icon.py"""
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "desktop" / "icon"
S = 1024
NAVY, PANEL, EDGE = (11, 21, 32), (16, 30, 44), (52, 80, 107)
GOLD, GOLD_DK, GOLD_LT = (233, 196, 106), (150, 112, 40), (255, 232, 170)
TEAL, PINK, BLUE = (60, 210, 190), (240, 110, 170), (90, 160, 255)


def font(size):
    for f in ("/System/Library/Fonts/Avenir Next.ttc", "/System/Library/Fonts/Helvetica.ttc"):
        try:
            return ImageFont.truetype(f, size, index=1 if f.endswith("Avenir Next.ttc") else 0)
        except OSError:
            pass
    return ImageFont.load_default()


def draw():
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    # the tile: macOS's rounded square, navy with a warm glow at the top right
    tile = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(tile).rounded_rectangle((100, 100, 924, 924), 185, fill=NAVY)
    glow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(glow).ellipse((430, -60, 1080, 590), fill=(233, 196, 106, 70))
    glow = glow.filter(ImageFilter.GaussianBlur(90))
    mask = tile.split()[3]
    tile = Image.alpha_composite(tile, Image.composite(glow, Image.new("RGBA", (S, S)), mask))
    shadow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle((100, 118, 924, 942), 185, fill=(0, 0, 0, 110))
    img = Image.alpha_composite(img, shadow.filter(ImageFilter.GaussianBlur(16)))
    img = Image.alpha_composite(img, tile)
    # the brass spyglass over the screen's corner: rim, knurled edge, smoked glass, a note on a staff
    cx, cy, R = 512, 512, 330
    rim = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    rd = ImageDraw.Draw(rim)
    rd.ellipse((cx - R - 10, cy - R + 6, cx + R + 10, cy + R + 26), fill=(0, 0, 0, 120))
    rim = rim.filter(ImageFilter.GaussianBlur(14))
    img = Image.alpha_composite(img, rim)
    d = ImageDraw.Draw(img)
    for k in range(R, R - 64, -1):          # brass: light at the top left, dark at the bottom right
        t = (R - k) / 64
        col = tuple(int(GOLD_DK[i] + (GOLD_LT[i] - GOLD_DK[i]) * (0.35 + 0.65 * (1 - abs(t - 0.4)))) for i in range(3))
        d.ellipse((cx - k, cy - k, cx + k, cy + k), fill=col)
    import math
    for a in range(0, 360, 6):              # the knurled edge
        x0 = cx + (R - 4) * math.cos(math.radians(a)); y0 = cy + (R - 4) * math.sin(math.radians(a))
        x1 = cx + (R - 16) * math.cos(math.radians(a)); y1 = cy + (R - 16) * math.sin(math.radians(a))
        d.line((x0, y0, x1, y1), fill=GOLD_DK, width=4)
    g = R - 64
    d.ellipse((cx - g, cy - g, cx + g, cy + g), fill=(14, 20, 26))
    for i in range(5):                      # a staff and one note on it
        y = cy - 78 + i * 39
        d.line((cx - 180, y, cx + 180, y), fill=(200, 210, 220), width=7)
    d.ellipse((cx - 45, cy - 21, cx + 39, cy + 39), fill=(245, 245, 250))
    d.line((cx + 33, cy + 9, cx + 33, cy - 180), fill=(245, 245, 250), width=14)
    return img


def main():
    OUT.mkdir(exist_ok=True)
    img = draw()
    img.save(OUT / "icon-1024.png")
    with tempfile.TemporaryDirectory() as t:
        iconset = Path(t) / "AppIcon.iconset"
        iconset.mkdir()
        for size in (16, 32, 128, 256, 512):
            img.resize((size, size), Image.LANCZOS).save(iconset / f"icon_{size}x{size}.png")
            img.resize((size * 2, size * 2), Image.LANCZOS).save(iconset / f"icon_{size}x{size}@2x.png")
        subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(OUT / "AppIcon.icns")], check=True)
    print(OUT / "AppIcon.icns")


if __name__ == "__main__":
    main()
