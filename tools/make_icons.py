#!/usr/bin/env python3
"""Generates the application icon set: PNGs, a Windows .ico and a macOS .icns.

Adapted from VectorEffects' `tools/make_icons.py`. No image library: the icon
is rasterised from signed distance fields and written as raw PNG, and the .ico
and .icns containers wrap those PNGs. The motif is a polar curve over speed
rings. Regenerate with:

    python3 tools/make_icons.py
"""
import math
import struct
import zlib
from pathlib import Path

SIZE = 512
BG = (0x0B, 0x2E, 0x33)
RING = (0x2F, 0x6B, 0x70)
CURVE = (0xFF, 0xB0, 0x4D)
ORIGIN = (0.34, 0.50)  # the polar's pole, in unit coordinates (y up)


def sd_round_box(px, py, half, radius):
    dx = abs(px) - half + radius
    dy = abs(py) - half + radius
    outside = math.hypot(max(dx, 0.0), max(dy, 0.0))
    return outside + min(max(dx, dy), 0.0) - radius


def sd_segment(px, py, ax, ay, bx, by):
    pax, pay = px - ax, py - ay
    bax, bay = bx - ax, by - ay
    denom = bax * bax + bay * bay
    h = 0.0 if denom == 0 else max(0.0, min(1.0, (pax * bax + pay * bay) / denom))
    return math.hypot(pax - bax * h, pay - bay * h)


def polar_point(radius, twa_deg):
    """TWA 0 points up, 180 down, the curve bulging to the right."""
    a = math.radians(twa_deg)
    return ORIGIN[0] + radius * math.sin(a), ORIGIN[1] + radius * math.cos(a)


def boat_speed(twa_deg):
    """A plausible polar shape: nothing head to wind, peak near 110."""
    if twa_deg < 32.0:
        return 0.0
    t = math.radians(twa_deg)
    return 0.44 * (0.72 + 0.28 * math.sin(t)) * (1.0 - math.exp(-(twa_deg - 32.0) / 14.0))


def polyline(points):
    return list(zip(points, points[1:]))


def strokes():
    """(segments, half-thickness, colour), painted in order."""
    rings = []
    for radius in (0.18, 0.30, 0.42):
        pts = [polar_point(radius, a) for a in range(0, 181, 6)]
        rings += polyline(pts)
    spokes = [(ORIGIN, polar_point(0.42, a)) for a in (0, 45, 90, 135, 180)]
    curve = polyline([polar_point(boat_speed(a), a) for a in range(30, 181, 3)])
    return [(rings + spokes, 0.010, RING), (curve, 0.030, CURVE)]


def render(size):
    unit = 1.0 / size
    aa = 1.2 * unit
    rgba = [[list(BG) + [0.0] for _ in range(size)] for _ in range(size)]
    for j in range(size):
        y = 1.0 - (j + 0.5) * unit
        for i in range(size):
            x = (i + 0.5) * unit
            d = sd_round_box(x - 0.5, y - 0.5, 0.5, 0.22)
            rgba[j][i][3] = max(0.0, min(1.0, 0.5 - d / aa))
    for segments, half, colour in strokes():
        dist = {}
        for (ax, ay), (bx, by) in segments:
            pad = half + 2 * aa
            i0, i1 = int((min(ax, bx) - pad) * size), int((max(ax, bx) + pad) * size) + 1
            j0, j1 = int((1 - max(ay, by) - pad) * size), int((1 - min(ay, by) + pad) * size) + 1
            for j in range(max(j0, 0), min(j1, size)):
                y = 1.0 - (j + 0.5) * unit
                for i in range(max(i0, 0), min(i1, size)):
                    x = (i + 0.5) * unit
                    d = sd_segment(x, y, ax, ay, bx, by) - half
                    if d < dist.get((j, i), 1.0):
                        dist[(j, i)] = d
        for (j, i), d in dist.items():
            cov = max(0.0, min(1.0, 0.5 - d / aa))
            px = rgba[j][i]
            for c in range(3):
                px[c] = colour[c] * cov + px[c] * (1.0 - cov)
    return rgba


def downsample(rgba, factor):
    """Box filter, weighting colour by coverage so the rim does not darken."""
    size = len(rgba) // factor
    out = []
    for j in range(size):
        row = []
        for i in range(size):
            acc = [0.0, 0.0, 0.0, 0.0]
            for dj in range(factor):
                for di in range(factor):
                    r, g, b, a = rgba[j * factor + dj][i * factor + di]
                    acc[0] += r * a
                    acc[1] += g * a
                    acc[2] += b * a
                    acc[3] += a
            a = acc[3]
            n = factor * factor
            row.append([acc[0] / a, acc[1] / a, acc[2] / a, a / n] if a else [0, 0, 0, 0.0])
        out.append(row)
    return out


def png_bytes(rgba):
    size = len(rgba)
    raw = b"".join(
        b"\x00" + bytes(v for r, g, b, a in row for v in (round(r), round(g), round(b), round(a * 255)))
        for row in rgba
    )

    def chunk(tag, data):
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    return png


def ico_bytes(images):
    """Modern ICO wrapping PNG payloads. 0 in the size byte means 256."""
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries, payload = b"", b""
    for size, png in images:
        entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(png), offset)
        offset += len(png)
        payload += png
    return header + entries + payload


def icns_bytes(images):
    """ICNS with PNG payloads: ic07 is 128, ic08 256, ic09 512."""
    body = b"".join(tag + struct.pack(">I", 8 + len(png)) + png for tag, png in images)
    return b"icns" + struct.pack(">I", 8 + len(body)) + body


if __name__ == "__main__":
    out = Path(__file__).resolve().parent.parent / "crates/pe-app/icons"
    out.mkdir(parents=True, exist_ok=True)
    print(f"rendering {SIZE}x{SIZE}...")
    full = render(SIZE)
    png = {SIZE: png_bytes(full)}
    for size in (256, 128, 32):
        png[size] = png_bytes(downsample(full, SIZE // size))
    (out / "icon.png").write_bytes(png[512])
    (out / "128x128@2x.png").write_bytes(png[256])
    (out / "128x128.png").write_bytes(png[128])
    (out / "32x32.png").write_bytes(png[32])
    (out / "icon.ico").write_bytes(ico_bytes([(32, png[32]), (256, png[256])]))
    (out / "icon.icns").write_bytes(icns_bytes([(b"ic07", png[128]), (b"ic08", png[256]), (b"ic09", png[512])]))
    print("done")
