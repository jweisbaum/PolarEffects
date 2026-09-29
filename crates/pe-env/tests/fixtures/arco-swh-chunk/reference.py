"""Reference values for tests/ranges.rs, decoded with numcodecs (c-blosc),
independently of blosc.rs.

The chunk `1100000.0.0` is ARCO-ERA5's
significant_height_of_combined_wind_waves_and_swell at hour 1,100,000 since
1900-01-01 (2025-06-27T08Z), recorded exactly as served on 2026-09-28 from
gs://gcp-public-data-arco-era5/ar/full_37-1h-0p25deg-chunk-1.zarr-v3.

Run with numcodecs 0.12 and numpy:  python reference.py
"""
import math
import numpy as np
from numcodecs import Blosc

raw = open("1100000.0.0", "rb").read()
field = np.frombuffer(Blosc().decode(raw), dtype="<f4").reshape(721, 1440)

def bilinear(lat, lon):
    p = (90.0 - lat) / 0.25
    r = min(int(math.floor(p)), 720)
    fr = p - r
    r1 = min(r + 1, 720)
    if r1 == r:
        fr = 0.0
    q = (lon % 360.0) / 0.25
    c = int(math.floor(q)) % 1440
    fc = q - math.floor(q)
    c1 = (c + 1) % 1440
    corners = [field[r, c], field[r, c1], field[r1, c], field[r1, c1]]
    weights = [(1 - fr) * (1 - fc), (1 - fr) * fc, fr * (1 - fc), fr * fc]
    s = t = 0.0
    for v, w in zip(corners, weights):
        if np.isfinite(v) and w > 0:
            s += float(v) * w
            t += w
    return (s / t) if t > 1e-12 else None, [float(x) for x in corners]

for lat, lon in [(50.1, -4.9), (44.375, 13.1), (90.0, 10.0), (-10.0, 359.9), (-60.1, -40.0), (-89.9, 20.0)]:
    print(lat, lon, *bilinear(lat, lon))
