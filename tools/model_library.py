#!/usr/bin/env python3
"""Build the bundled model library (assets/models/library.zip).

Downloads a curated set of Kenney 3D packs (all CC0), keeps the models that
suit the app (no floor tiles, walls or road pieces; not too heavy), drops
exact geometric duplicates (colour variants look the same here, since the
app paints models with its own materials) and writes one zip:

    index.json          [{id, name, category, tris}, ...]
    <id>.glb            the model: positions, normals and triangles only

Run it by hand when the selection changes:

    python3 tools/model_library.py [--cache DIR]

Only the Python standard library is needed.
"""

import argparse
import hashlib
import io
import json
import re
import struct
import sys
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "assets" / "models" / "library.zip"
LICENSE_OUT = ROOT / "assets" / "models" / "LICENSE.txt"

# Kenney pack -> category shown in the app.
PACKS = {
    "space-kit": "Space",
    "space-station-kit": "Space",
    "modular-space-kit": "Space",
    "blaster-kit": "Space",
    "car-kit": "Vehicles",
    "racing-kit": "Vehicles",
    "toy-car-kit": "Vehicles",
    "train-kit": "Vehicles",
    "watercraft-kit": "Vehicles",
    "nature-kit": "Nature",
    "food-kit": "Food",
    "holiday-kit": "Holiday",
    "graveyard-kit": "Spooky",
    "pirate-kit": "Pirate",
    "castle-kit": "Fantasy",
    "retro-fantasy-kit": "Fantasy",
    "fantasy-town-kit": "Fantasy",
    "city-kit-commercial": "City",
    "city-kit-suburban": "City",
    "cube-pets": "Characters",
    "mini-characters": "Characters",
    "blocky-characters": "Characters",
    "mini-arcade": "Objects",
    "survival-kit": "Objects",
    "prototype-kit": "Objects",
}

# Building pieces that look like nothing on their own.
SKIP_NAME = re.compile(
    r"floor|wall|tile|road|corner|straight|ground|path|border|edge|slope|"
    r"stairs|step|curb|pavement|sidewalk|rail(?!gun)|track|fence|gate|"
    r"bridge|platform|base|terrain|water|cliff|split|junction|crossing|"
    r"intersection|end\b|_end|ramp|half|quarter|window|door|roof|column|"
    r"pillar|beam|support|panel|pipe|cable|hole|overlay|decal|shadow|grid|"
    r"debris|crops?_|stage[a-z]\b|dirt|snow|connector|part\b|pile|indicator|"
    r"selection|spawn",
    re.I,
)
MIN_TRIS = 12
MAX_TRIS = 20000
# Thickness / largest extent below this is a flat tile.
MIN_THICKNESS = 0.12


def fetch(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "EZ2DEMOSCENE-model-library"})
    with urllib.request.urlopen(req, timeout=120) as r:
        return r.read()


def pack_zip(slug: str, cache: Path) -> bytes:
    f = cache / f"{slug}.zip"
    if f.exists():
        return f.read_bytes()
    page = fetch(f"https://kenney.nl/assets/{slug}").decode("utf-8", "replace")
    m = re.search(r"https://kenney\.nl/media/pages/assets/[^\"']+\.zip", page)
    if not m:
        raise RuntimeError(f"no download link on the {slug} page")
    data = fetch(m.group(0))
    cache.mkdir(parents=True, exist_ok=True)
    f.write_bytes(data)
    return data


def read_glb(b: bytes):
    if b[:4] != b"glTF":
        raise ValueError("not a GLB")
    jlen = struct.unpack("<I", b[12:16])[0]
    doc = json.loads(b[20 : 20 + jlen])
    rest = b[20 + jlen :]
    blob = b""
    if len(rest) >= 8 and rest[4:8] == b"BIN\x00":
        blen = struct.unpack("<I", rest[:4])[0]
        blob = rest[8 : 8 + blen]
    return doc, blob


def write_glb(doc, blob: bytes) -> bytes:
    j = json.dumps(doc, separators=(",", ":")).encode()
    j += b" " * (-len(j) % 4)
    blob += b"\x00" * (-len(blob) % 4)
    chunks = struct.pack("<I", len(j)) + b"JSON" + j
    if blob:
        chunks += struct.pack("<I", len(blob)) + b"BIN\x00" + blob
    return b"glTF" + struct.pack("<II", 2, 12 + len(chunks)) + chunks


def strip_images(doc):
    """The app draws models with its own materials: drop textures."""
    for k in ("images", "textures", "samplers"):
        doc.pop(k, None)
    for mat in doc.get("materials", []):
        pbr = mat.get("pbrMetallicRoughness", {})
        for k in ("baseColorTexture", "metallicRoughnessTexture"):
            pbr.pop(k, None)
        for k in ("normalTexture", "occlusionTexture", "emissiveTexture"):
            mat.pop(k, None)
    return doc


def repack(doc, blob: bytes):
    """Keep only positions, normals and triangle indices (all the app reads),
    tightly packed in a new buffer."""
    out = bytearray()
    views, accessors = [], []

    def take(i):
        acc = doc["accessors"][i]
        bv = doc["bufferViews"][acc["bufferView"]]
        comps = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[acc["type"]]
        csize = {5120: 1, 5121: 1, 5122: 2, 5123: 2, 5125: 4, 5126: 4}[acc["componentType"]]
        elem = comps * csize
        stride = bv.get("byteStride", elem)
        start = bv.get("byteOffset", 0) + acc.get("byteOffset", 0)
        data = b"".join(
            blob[start + k * stride : start + k * stride + elem] for k in range(acc["count"])
        )
        out.extend(b"\x00" * (-len(out) % 4))
        views.append({"buffer": 0, "byteOffset": len(out), "byteLength": len(data)})
        out.extend(data)
        new = {k: v for k, v in acc.items() if k not in ("bufferView", "byteOffset", "sparse")}
        new["bufferView"] = len(views) - 1
        accessors.append(new)
        return len(accessors) - 1

    for mesh in doc.get("meshes", []):
        for prim in mesh["primitives"]:
            attrs = prim["attributes"]
            prim["attributes"] = {k: take(attrs[k]) for k in ("POSITION", "NORMAL") if k in attrs}
            if "indices" in prim:
                prim["indices"] = take(prim["indices"])
            for k in ("material", "targets"):
                prim.pop(k, None)
    for k in ("materials", "animations", "skins", "cameras", "extensionsUsed",
              "extensionsRequired", "extras"):
        doc.pop(k, None)
    for node in doc.get("nodes", []):
        node.pop("skin", None)
    doc["accessors"], doc["bufferViews"] = accessors, views
    doc["buffers"] = [{"byteLength": len(out)}]
    return doc, bytes(out)


def stats(doc, blob: bytes):
    """Triangle count, bounding box and a hash of the geometry."""
    tris = 0
    lo = [float("inf")] * 3
    hi = [float("-inf")] * 3
    h = hashlib.sha1()
    for mesh in doc.get("meshes", []):
        for prim in mesh["primitives"]:
            if prim.get("mode", 4) != 4:
                continue
            acc = doc["accessors"][prim["attributes"]["POSITION"]]
            if "min" in acc and "max" in acc:
                lo = [min(a, b) for a, b in zip(lo, acc["min"])]
                hi = [max(a, b) for a, b in zip(hi, acc["max"])]
            idx = prim.get("indices")
            tris += (doc["accessors"][idx]["count"] if idx is not None else acc["count"]) // 3
            bv = doc["bufferViews"][acc["bufferView"]]
            start = bv.get("byteOffset", 0) + acc.get("byteOffset", 0)
            h.update(blob[start : start + acc["count"] * 12])
    size = [b - a for a, b in zip(lo, hi)]
    return tris, size, h.hexdigest()


def pretty(name: str) -> str:
    name = re.sub(r"[-_]+", " ", name)
    name = re.sub(r"(?<=[a-z])(?=[A-Z0-9])", " ", name)
    return name[:1].upper() + name[1:]


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--cache", type=Path, default=ROOT / "target" / "kenney-cache")
    args = ap.parse_args()

    entries, seen = [], set()
    skipped = {"name": 0, "flat": 0, "tris": 0, "dup": 0, "bad": 0}
    licenses = []
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for slug, category in PACKS.items():
            try:
                data = pack_zip(slug, args.cache)
            except Exception as e:  # noqa: BLE001 - report and carry on
                print(f"  {slug}: {e}", file=sys.stderr)
                continue
            kept = 0
            with zipfile.ZipFile(io.BytesIO(data)) as pack:
                for info in sorted(pack.infolist(), key=lambda i: i.filename):
                    fn = info.filename
                    if fn.lower().startswith("license") or fn.lower().endswith("license.txt"):
                        if not any(l[0] == slug for l in licenses):
                            licenses.append((slug, pack.read(info).decode("utf-8", "replace")))
                    if not fn.lower().endswith(".glb") or "__MACOSX" in fn:
                        continue
                    stem = Path(fn).stem
                    if SKIP_NAME.search(stem):
                        skipped["name"] += 1
                        continue
                    try:
                        doc, blob = read_glb(pack.read(info))
                        tris, size, geo = stats(doc, blob)
                    except Exception:  # noqa: BLE001
                        skipped["bad"] += 1
                        continue
                    if not MIN_TRIS <= tris <= MAX_TRIS:
                        skipped["tris"] += 1
                        continue
                    if min(size) < MIN_THICKNESS * max(size):
                        skipped["flat"] += 1
                        continue
                    if geo in seen:
                        skipped["dup"] += 1
                        continue
                    seen.add(geo)
                    mid = f"kenney/{slug}/{stem}"
                    z.writestr(f"{mid}.glb", write_glb(*repack(strip_images(doc), blob)))
                    entries.append(
                        {"id": mid, "name": pretty(stem), "category": category, "tris": tris}
                    )
                    kept += 1
            print(f"{slug:24} {kept:4} models")
        z.writestr("index.json", json.dumps(entries, indent=0))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(out.getvalue())
    with open(LICENSE_OUT, "w") as f:
        f.write(
            "The models in library.zip come from Kenney (www.kenney.nl) and are\n"
            "released under Creative Commons Zero (CC0):\n"
            "http://creativecommons.org/publicdomain/zero/1.0/\n\n"
            "Packs: " + ", ".join(PACKS) + "\n"
        )
    by_cat = {}
    for e in entries:
        by_cat[e["category"]] = by_cat.get(e["category"], 0) + 1
    print(f"\n{len(entries)} models, {OUT.stat().st_size / 1e6:.1f} MB -> {OUT.relative_to(ROOT)}")
    print("by category:", by_cat)
    print("skipped:", skipped)


if __name__ == "__main__":
    main()
