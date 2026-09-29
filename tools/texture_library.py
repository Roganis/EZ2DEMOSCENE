#!/usr/bin/env python3
"""Build the bundled texture library (assets/textures/texture_library.zip).

Two kinds of seamless textures, all CC0:

* PBR materials from Poly Haven: colour, normal map (OpenGL) and
  occlusion / roughness / metalness packed as glTF does, 256 × 256.
* Low-resolution textures from Screaming Brain Studios' texture packs and
  Kenney's voxel and pattern packs, at most 64 × 64 with at most 128 colours.

Every picture is checked to tile (its opposite edges must meet no worse
than neighbouring pixels differ inside it) and exact duplicates are
dropped. The zip holds:

    index.json          [{id, name, category, kind, size}, ...]
    <id>.jpg            PBR colour        (kind "pbr")
    <id>.normal.jpg     PBR normal map
    <id>.orm.jpg        PBR occlusion / roughness / metalness
    <id>.png            low-res texture   (kind "tile")

Run it by hand when the selection changes:

    python3 tools/texture_library.py [--cache DIR]

Needs Pillow.
"""

import argparse
import concurrent.futures
import hashlib
import io
import json
import re
import sys
import urllib.request
import zipfile
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "assets" / "textures" / "texture_library.zip"
LICENSE_OUT = ROOT / "assets" / "textures" / "TEXTURE_LIBRARY_LICENSE.txt"

PBR_SIZE = 256
TILE_SIZE = 64
TILE_COLORS = 128
# Poly Haven materials per category (the most downloaded ones).
PBR_PER_CATEGORY = 14
# Edges may differ this much more than neighbouring pixels do inside.
MAX_SEAM = 2.2

# Poly Haven category -> category shown in the app (first match wins).
PBR_CATEGORIES = [
    ("brick", "Brick"),
    ("roofing", "Roofing"),
    ("metal", "Metal"),
    ("bark", "Bark"),
    ("wood", "Wood"),
    ("fabric", "Fabric"),
    ("leather", "Fabric"),
    ("cobblestone", "Paving"),
    ("tiles", "Paving"),
    ("rock", "Rock"),
    ("sandstone", "Rock"),
    ("snow", "Ground"),
    ("sand", "Ground"),
    ("gravel", "Ground"),
    ("terrain", "Ground"),
    ("asphalt", "Concrete & plaster"),
    ("concrete", "Concrete & plaster"),
    ("plaster", "Concrete & plaster"),
    ("plaster-concrete", "Concrete & plaster"),
]

SBS = "https://opengameart.org/sites/default/files/"
# Low-res packs: (id prefix, url, credit).
TILE_PACKS = [
    ("sbs/tiny1", SBS + "sbs_-_tiny_texture_pack_-_128x128.zip", "Screaming Brain Studios: Tiny Texture Pack"),
    ("sbs/tiny2", SBS + "sbs_-_tiny_texture_pack_2_-_128x128.zip", "Screaming Brain Studios: Tiny Texture Pack 2"),
    ("sbs/synth", SBS + "sbs_-_synthetic_texture_pack_-_128x128_0.zip", "Screaming Brain Studios: Synthetic Texture Pack"),
    ("sbs/horror", SBS + "sbs_-_horror_texture_pack_128x128.zip", "Screaming Brain Studios: Horror Texture Pack"),
    ("sbs/floors", SBS + "sbs_-_mini_texture_pack_1_-_floor_tiles_128x128.zip", "Screaming Brain Studios: 200 Tile Floor Textures"),
    ("sbs/roofs", SBS + "sbs_-_mini_texture_pack_2_-_rooftops_128x128.zip", "Screaming Brain Studios: 150 Roof Textures"),
    ("sbs/liquids", SBS + "sbs_-_mini_texture_pack_3_-_liquids_128x128.zip", "Screaming Brain Studios: 140 Liquid Textures"),
    ("kenney/voxel", "kenney:voxel-pack", "Kenney: Voxel Pack"),
    ("kenney/patterns", "kenney:pattern-pack-pixel", "Kenney: Pattern Pack (Pixel)"),
]

# Low-res picture name / folder -> category shown in the app.
TILE_CATEGORIES = [
    (r"kenney/patterns", "Patterns"),
    (r"liquid|water|lava", "Liquids"),
    (r"roof|thatch", "Roofs"),
    (r"brick", "Brick"),
    (r"wood|plank|trunk|tree|bark|table", "Wood"),
    (r"metal|iron|plate", "Metal"),
    (r"grass|dirt|ground|sand|snow|gravel|leaves|moss|cactus|mushroom|wheat", "Ground"),
    (r"stone|rock|coal|diamond|gold|ruby|emerald|silver", "Stone"),
    (r"tile|floor|checker|diamond|rectangle", "Floor tiles"),
    (r"plaster|wall|stain", "Walls"),
    (r"fabric|leather|cotton", "Fabric"),
    (r"horror|misc|elements|synth", "Odd"),
]
# Voxel-pack pieces that aren't textures.
TILE_SKIP = re.compile(r"(dirt|stone|greystone|redstone)_(grass|sand|snow|dirt)|track|fence|glass|oven|_top\b|_inside|transparent|stage", re.I)


def fetch(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "EZ2DEMOSCENE-texture-library"})
    with urllib.request.urlopen(req, timeout=120) as r:
        return r.read()


def cached(cache: Path, name: str, url: str) -> bytes:
    f = cache / name
    if f.exists():
        return f.read_bytes()
    data = fetch(url)
    f.parent.mkdir(parents=True, exist_ok=True)
    f.write_bytes(data)
    return data


def kenney_zip(slug: str, cache: Path) -> bytes:
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


def seam_ratio(img: Image.Image) -> float:
    """How much worse the wrap-around edges match than most neighbouring
    rows and columns inside (about 1 = no visible seam; mortar lines and
    such inside count as ordinary, so one at the edge passes too)."""
    g = img.convert("L")
    w, h = g.size
    px = g.load()

    def ratio(get, n, m):
        cols = [sum(abs(get(i, j) - get((i + 1) % n, j)) for j in range(m)) / m for i in range(n)]
        seam = cols[-1]
        inner = sorted(cols[:-1])
        return seam / max(inner[int(len(inner) * 0.95)], 0.5)

    return max(
        ratio(lambda i, j: px[i, j], w, h),
        ratio(lambda i, j: px[j, i], h, w),
    )


def shrink(img: Image.Image, size: int) -> Image.Image:
    """Downscale a square picture that tiles so it still tiles: an exact
    box filter when the size divides, else a wrap-padded resample."""
    w, h = img.size
    if (w, h) == (size, size):
        return img
    if w % size == 0 and h % size == 0:
        return img.reduce((w // size, h // size))
    pad = max(w, h) // 8
    big = Image.new(img.mode, (w + 2 * pad, h + 2 * pad))
    for dx in (-1, 0, 1):
        for dy in (-1, 0, 1):
            big.paste(img, (pad + dx * w, pad + dy * h))
    k = size / w
    out = big.resize((round(big.width * k), round(big.height * k)), Image.LANCZOS)
    p = round(pad * k)
    return out.crop((p, p, p + size, p + size))


def jpg(img: Image.Image, quality: int) -> bytes:
    b = io.BytesIO()
    img.convert("RGB").save(b, "JPEG", quality=quality, optimize=True, progressive=False)
    return b.getvalue()


def png(img: Image.Image) -> bytes:
    b = io.BytesIO()
    img.save(b, "PNG", optimize=True)
    return b.getvalue()


def pretty(name: str) -> str:
    name = re.sub(r"[-_]+", " ", name)
    name = re.sub(r"(?<=[a-z])(?=[A-Z0-9])", " ", name)
    name = re.sub(r"\s+", " ", name).strip()
    return name[:1].upper() + name[1:]


# --- Poly Haven ---------------------------------------------------------------


def pbr_category(cats):
    for key, label in PBR_CATEGORIES:
        if key in cats:
            return label
    return None


def pbr_material(slug: str, cache: Path):
    """(colour, normal, orm) pictures of a Poly Haven texture at PBR_SIZE."""
    files = json.loads(cached(cache, f"polyhaven/{slug}.json", f"https://api.polyhaven.com/files/{slug}"))

    def get(key):
        entry = files.get(key, {}).get("1k", {}).get("jpg") or files.get(key, {}).get("1k", {}).get("png")
        if not entry:
            return None
        url = entry["url"]
        data = cached(cache, f"polyhaven/{slug}/{url.rsplit('/', 1)[1]}", url)
        return Image.open(io.BytesIO(data))

    color = get("Diffuse")
    normal = get("nor_gl")
    if color is None or normal is None:
        raise RuntimeError("no colour or normal map")
    arm = get("arm")
    if arm is None:
        rough = get("Rough")
        if rough is None:
            raise RuntimeError("no roughness")
        ao = get("AO")
        metal = get("Metal")
        size = rough.size
        black = Image.new("L", size, 0)
        white = Image.new("L", size, 255)
        arm = Image.merge(
            "RGB",
            (
                ao.convert("L").resize(size) if ao else white,
                rough.convert("L"),
                metal.convert("L").resize(size) if metal else black,
            ),
        )
    out = []
    for img in (color, normal, arm):
        img = img.convert("RGB")
        if img.width != img.height:
            raise RuntimeError("not square")
        out.append(shrink(img, PBR_SIZE))
    return out


def polyhaven(cache: Path, add):
    assets = json.loads(cached(cache, "polyhaven/assets.json", "https://api.polyhaven.com/assets?t=textures"))
    buckets = {}
    for slug, a in assets.items():
        cat = pbr_category(a.get("categories", []))
        if cat is None or "aerial" in a.get("categories", []):
            continue
        buckets.setdefault(cat, []).append((a.get("download_count", 0), slug, a["name"]))
    chosen = []
    for cat, items in buckets.items():
        items.sort(reverse=True)
        chosen += [(cat, slug, name) for _, slug, name in items[: PBR_PER_CATEGORY * 2]]

    def work(item):
        cat, slug, name = item
        try:
            return item, pbr_material(slug, cache), None
        except Exception as e:  # noqa: BLE001 - report and carry on
            return item, None, e

    kept = {}
    with concurrent.futures.ThreadPoolExecutor(8) as pool:
        for (cat, slug, name), maps, err in pool.map(work, chosen):
            if err is not None:
                print(f"  polyhaven {slug}: {err}", file=sys.stderr)
                continue
            if kept.get(cat, 0) >= PBR_PER_CATEGORY:
                continue
            if seam_ratio(maps[0]) > MAX_SEAM:
                print(f"  polyhaven {slug}: does not tile", file=sys.stderr)
                continue
            color, normal, orm = maps
            if add(f"polyhaven/{slug}", name, cat, "pbr", {
                ".jpg": jpg(color, 85),
                ".normal.jpg": jpg(normal, 85),
                ".orm.jpg": jpg(orm, 85),
            }):
                kept[cat] = kept.get(cat, 0) + 1
    print(f"polyhaven               {sum(kept.values()):4} materials {kept}")


# --- low-res packs ------------------------------------------------------------


def tile_category(path: str):
    for pattern, label in TILE_CATEGORIES:
        if re.search(pattern, path, re.I):
            return label
    return "Odd"


def tile_picture(img: Image.Image) -> Image.Image:
    img = img.convert("RGBA")
    if img.getextrema()[3][0] < 250:
        raise RuntimeError("see-through")
    img = img.convert("RGB")
    if img.width != img.height:
        raise RuntimeError("not square")
    if img.width > TILE_SIZE:
        img = shrink(img, TILE_SIZE)
    return img.quantize(TILE_COLORS, dither=Image.Dither.NONE).convert("RGB")


def tile_packs(cache: Path, add):
    for prefix, url, _credit in TILE_PACKS:
        try:
            if url.startswith("kenney:"):
                data = kenney_zip(url.split(":", 1)[1], cache)
            else:
                data = cached(cache, url.rsplit("/", 1)[1], url)
        except Exception as e:  # noqa: BLE001
            print(f"  {prefix}: {e}", file=sys.stderr)
            continue
        kept = skipped = 0
        with zipfile.ZipFile(io.BytesIO(data)) as pack:
            for info in sorted(pack.infolist(), key=lambda i: i.filename):
                fn = info.filename
                if not fn.lower().endswith(".png") or "__MACOSX" in fn:
                    continue
                if prefix == "kenney/voxel" and "/Tiles/" not in fn:
                    continue
                if prefix == "kenney/patterns" and "Tiles (Color)/" not in fn:
                    continue
                stem = Path(fn).stem
                if TILE_SKIP.search(stem):
                    skipped += 1
                    continue
                try:
                    img = tile_picture(Image.open(io.BytesIO(pack.read(info))))
                except Exception:  # noqa: BLE001
                    skipped += 1
                    continue
                if seam_ratio(img) > MAX_SEAM:
                    skipped += 1
                    continue
                clean = re.sub(r"\s*-\s*\d+x\d+$", "", stem)
                clean = re.sub(r"[-_]\d+x\d+$", "", clean)
                slug = re.sub(r"[^A-Za-z0-9]+", "_", clean).strip("_").lower()
                folder = Path(fn).parent.name
                cat = tile_category(f"{prefix}/{folder}/{stem}")
                name = pretty(clean.replace("Synth-", "").replace("Horror_", "Horror "))
                if add(f"{prefix}/{slug}", name, cat, "tile", {".png": png(img)}):
                    kept += 1
        print(f"{prefix:24} {kept:4} textures ({skipped} skipped)")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--cache", type=Path, default=ROOT / "target" / "tex-cache")
    args = ap.parse_args()

    entries, seen = [], set()
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:

        def add(tid, name, category, kind, files):
            h = hashlib.sha1(b"".join(files[k] for k in sorted(files))).hexdigest()
            if h in seen or any(e["id"] == tid for e in entries):
                return False
            seen.add(h)
            for ext, data in files.items():
                # JPEG and PNG are compressed already.
                z.writestr(zipfile.ZipInfo(f"{tid}{ext}", (2026, 1, 1, 0, 0, 0)), data)
            size = PBR_SIZE if kind == "pbr" else Image.open(io.BytesIO(next(iter(files.values())))).width
            entries.append({"id": tid, "name": name, "category": category, "kind": kind, "size": size})
            return True

        polyhaven(args.cache, add)
        tile_packs(args.cache, add)
        z.writestr("index.json", json.dumps(entries, indent=0))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(out.getvalue())
    with open(LICENSE_OUT, "w") as f:
        f.write(
            "The textures in texture_library.zip are released under Creative Commons Zero\n"
            "(CC0): http://creativecommons.org/publicdomain/zero/1.0/\n\n"
            "PBR materials: Poly Haven (polyhaven.com)\n"
            "Low-resolution textures:\n"
            + "".join(f"  {credit}\n" for _, _, credit in TILE_PACKS)
        )
    counts = {}
    for e in entries:
        k = (e["kind"], e["category"])
        counts[k] = counts.get(k, 0) + 1
    print(f"\n{len(entries)} textures, {OUT.stat().st_size / 1e6:.1f} MB -> {OUT.relative_to(ROOT)}")
    for k in sorted(counts):
        print(f"  {k[0]:5} {k[1]:20} {counts[k]}")


if __name__ == "__main__":
    main()
