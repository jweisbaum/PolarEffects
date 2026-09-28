"""Records cropped current fixtures for pe-env (M9).

For each store: fetch .zmetadata, the axis arrays, and the one variable chunk
holding the test position over 2025-08-02T00Z..+48h; decode with numcodecs (c-blosc);
crop the time dimension to those 48 hours; re-encode with the same codec;
write a zarr v2 folder with .zarray shapes cut to match. The corner values
it prints into refs.json are the independent reference `tests/sampler.rs`
asserts against.

    uv run --with numcodecs==0.12.1 --with numpy python record.py <out dir>

Recorded 2026-09-28 (6.7 MB downloaded) at 49.86N 5.13W, a place whose
bilinear stencil sits inside one chunk of all three stores.
"""
import json, os, sys, urllib.request, datetime
import numpy as np
import numcodecs

BASE = "https://s3.waw3-1.cloudferro.com/"
STORES = {
  "nws": ("mdl-arco-geo-041/arco/NWSHELF_MULTIYEAR_PHY_004_009/cmems_mod_nws_phy-uv_my_7km-2D_PT1H-i_202112/geoChunked.zarr", ["uo", "vo"]),
  "merged": ("mdl-arco-geo-015/arco/GLOBAL_ANALYSISFORECAST_PHY_001_024/cmems_mod_glo_phy_anfc_merged-uv_PT1H-i_202211/geoChunked.zarr", ["uo", "vo", "utide", "vtide", "vsdx", "vsdy"]),
  "gc-my": ("mdl-arco-geo-037/arco/MULTIOBS_GLO_PHY_MYNRT_015_003/cmems_obs-mob_glo_phy-cur_my_0.25deg_PT1H-i_202411/geoChunked.zarr", ["uo", "vo"]),
}
T0 = int(datetime.datetime(2025, 8, 2, tzinfo=datetime.timezone.utc).timestamp())
HOURS = 48
LAT, LON = 49.86, -5.13
fetched = 0

def get(url):
    global fetched
    with urllib.request.urlopen(url, timeout=120) as r:
        b = r.read()
    fetched += len(b)
    return b

def codec(meta):
    return numcodecs.get_codec(meta["compressor"])

def decode(meta, raw):
    arr = np.frombuffer(codec(meta).decode(raw), dtype=np.dtype(meta["dtype"]))
    return arr.reshape(meta["chunks"])

def encode(meta, arr):
    return codec(meta).encode(np.ascontiguousarray(arr.astype(np.dtype(meta["dtype"]))).tobytes())

def epoch_units(units):
    unit, since = units.split(" since ")
    since = since.replace("T", " ").replace("+00:00", "").strip()
    fmt = "%Y-%m-%d %H:%M:%S" if ":" in since else "%Y-%m-%d"
    e = int(datetime.datetime.strptime(since, fmt).replace(tzinfo=datetime.timezone.utc).timestamp())
    return {"seconds": 1, "hours": 3600, "days": 86400}[unit.strip()], e

out_root = sys.argv[1]
refs = {}
for name, (path, variables) in STORES.items():
    url = BASE + path + "/"
    meta = json.loads(get(url + ".zmetadata"))["metadata"]
    out = os.path.join(out_root, name)
    os.makedirs(out, exist_ok=True)
    json.dump({"zarr_format": 2}, open(os.path.join(out, ".zgroup"), "w"))
    json.dump(meta.get(".zattrs", {}), open(os.path.join(out, ".zattrs"), "w"), indent=1)
    axes = {}
    for ax in ["latitude", "longitude", "elevation"]:
        if ax + "/.zarray" not in meta:
            continue
        am = meta[ax + "/.zarray"]
        os.makedirs(os.path.join(out, ax), exist_ok=True)
        json.dump(am, open(os.path.join(out, ax, ".zarray"), "w"), indent=1)
        json.dump(meta[ax + "/.zattrs"], open(os.path.join(out, ax, ".zattrs"), "w"), indent=1)
        raw = get(url + ax + "/0")
        open(os.path.join(out, ax, "0"), "wb").write(raw)
        axes[ax] = decode(am, raw)[: am["shape"][0]].astype(float)
    # time
    tm = meta["time/.zarray"]; ta = meta["time/.zattrs"]
    per, epoch = epoch_units(ta["units"])
    first_chunk = decode(tm, get(url + "time/0"))
    first = epoch + int(first_chunk[0]) * per
    step = (int(first_chunk[1]) - int(first_chunk[0])) * per
    t_index = (T0 - first) // step
    assert (T0 - first) % step == 0
    tvals = np.array([ (T0 + k * 3600 - epoch) / per for k in range(HOURS)])
    ntm = dict(tm); ntm["shape"] = [HOURS]; ntm["chunks"] = [HOURS]
    os.makedirs(os.path.join(out, "time"), exist_ok=True)
    json.dump(ntm, open(os.path.join(out, "time", ".zarray"), "w"), indent=1)
    json.dump(ta, open(os.path.join(out, "time", ".zattrs"), "w"), indent=1)
    open(os.path.join(out, "time", "0"), "wb").write(encode(ntm, tvals))
    lat, lon = axes["latitude"], axes["longitude"]
    for var in variables:
        vm = meta[var + "/.zarray"]; va = meta[var + "/.zattrs"]
        dims = va["_ARRAY_DIMENSIONS"]
        chunks = vm["chunks"]
        ct = t_index // chunks[0]; off = t_index % chunks[0]
        assert off + HOURS <= chunks[0], "window crosses a time chunk"
        li = int(np.floor((LAT - lat[0]) / (lat[1] - lat[0])))
        lj = int(np.floor((LON - lon[0]) / (lon[1] - lon[0])))
        cl, cw = li // chunks[-2], lj // chunks[-1]
        assert li % chunks[-2] + 1 < chunks[-2] and lj % chunks[-1] + 1 < chunks[-1], "stencil crosses a chunk"
        mid = []
        if len(dims) == 4:
            ev = axes.get("elevation")
            m = int(np.argmin(np.abs(ev))) if ev is not None else 0
            mid = [m // chunks[1]]
        key = ".".join(str(x) for x in [ct] + mid + [cl, cw])
        raw = get(url + var + "/" + key)
        data = decode(vm, raw)
        crop = data[off: off + HOURS]
        nvm = dict(vm); nvm["shape"] = [HOURS] + vm["shape"][1:]; nvm["chunks"] = [HOURS] + chunks[1:]
        os.makedirs(os.path.join(out, var), exist_ok=True)
        json.dump(nvm, open(os.path.join(out, var, ".zarray"), "w"), indent=1)
        json.dump(va, open(os.path.join(out, var, ".zattrs"), "w"), indent=1)
        nkey = ".".join(str(x) for x in [0] + mid + [cl, cw])
        open(os.path.join(out, var, nkey), "wb").write(encode(nvm, crop))
        # reference: the 2x2 raw corner values at hours 24 and 25 (2025-08-03T00Z, 01Z)
        c = crop.reshape([HOURS] + ([1] if mid else []) + chunks[-2:])
        if mid: c = c[:, 0]
        r0, c0 = li % chunks[-2], lj % chunks[-1]
        refs[f"{name}/{var}"] = {
            "key": key, "lat_index": li, "lon_index": lj,
            "lat": [lat[li], lat[li + 1]], "lon": [lon[lj], lon[lj + 1]],
            "scale": va.get("scale_factor", 1.0), "fill": vm["fill_value"],
            "corners_h24": c[24, r0:r0 + 2, c0:c0 + 2].tolist(),
            "corners_h25": c[25, r0:r0 + 2, c0:c0 + 2].tolist(),
        }
json.dump(refs, open(os.path.join(out_root, "refs.json"), "w"), indent=1)
print("fetched bytes", fetched)
