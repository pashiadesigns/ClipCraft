"""Turns the headless Help scenes into the pictures shipped with ClipCraft.

Input is the folder written by the help gallery test: one subfolder per
picture with egui meshes, its texture atlases and spec.json (crop rectangle
and rings). Output is one PNG per picture and the pack the app embeds.
No window, desktop capture or user data is involved.
"""
import json
import struct
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

BACKGROUND = (19 / 255, 22 / 255, 22 / 255, 1)
RING = (242, 201, 76, 255)
SCALE = 2


def rasterize(root, crop):
    x, y, w, h = crop
    out = np.zeros((h * SCALE, w * SCALE, 4), dtype=np.float32)
    out[:] = BACKGROUND
    textures = {}
    for mesh in json.loads((root / 'meshes.json').read_text()):
        name = mesh['texture']
        if name not in textures:
            textures[name] = np.asarray(Image.open(root / name).convert('RGBA'), dtype=np.float32) / 255
        tex = textures[name]
        vertices = mesh['vertices']
        clip = mesh['clip']
        for i in range(0, len(mesh['indices']), 3):
            v = [vertices[j] for j in mesh['indices'][i:i + 3]]
            p = (np.array([a[:2] for a in v], dtype=np.float32) - [x, y]) * SCALE
            x0 = max(0, int(np.floor(p[:, 0].min())), int(np.ceil((clip[0] - x) * SCALE)))
            y0 = max(0, int(np.floor(p[:, 1].min())), int(np.ceil((clip[1] - y) * SCALE)))
            x1 = min(w * SCALE, int(np.ceil(p[:, 0].max())), int(np.ceil((clip[2] - x) * SCALE)))
            y1 = min(h * SCALE, int(np.ceil(p[:, 1].max())), int(np.ceil((clip[3] - y) * SCALE)))
            if x0 >= x1 or y0 >= y1:
                continue
            den = (p[1, 1] - p[2, 1]) * (p[0, 0] - p[2, 0]) + (p[2, 0] - p[1, 0]) * (p[0, 1] - p[2, 1])
            if abs(den) < 1e-8:
                continue
            py, px = np.mgrid[y0:y1, x0:x1].astype(np.float32)
            px += .5
            py += .5
            b0 = ((p[1, 1] - p[2, 1]) * (px - p[2, 0]) + (p[2, 0] - p[1, 0]) * (py - p[2, 1])) / den
            b1 = ((p[2, 1] - p[0, 1]) * (px - p[2, 0]) + (p[0, 0] - p[2, 0]) * (py - p[2, 1])) / den
            b2 = 1 - b0 - b1
            valid = (b0 >= -1e-4) & (b1 >= -1e-4) & (b2 >= -1e-4)
            if not valid.any():
                continue
            bary = np.stack([b0, b1, b2], axis=-1)
            uv = bary @ np.array([a[2:4] for a in v])
            tx = np.clip(uv[..., 0] * tex.shape[1] - .5, 0, tex.shape[1] - 1)
            ty = np.clip(uv[..., 1] * tex.shape[0] - .5, 0, tex.shape[0] - 1)
            ix, iy = tx.astype(int), ty.astype(int)
            jx, jy = np.minimum(ix + 1, tex.shape[1] - 1), np.minimum(iy + 1, tex.shape[0] - 1)
            fx, fy = (tx - ix)[..., None], (ty - iy)[..., None]
            sample = (tex[iy, ix] * (1 - fx) + tex[iy, jx] * fx) * (1 - fy) + (tex[jy, ix] * (1 - fx) + tex[jy, jx] * fx) * fy
            colors = bary @ (np.array([a[4] for a in v], dtype=np.float32) / 255)
            src = sample * colors
            dest = out[y0:y1, x0:x1]
            dest[valid] = (src + dest * (1 - src[..., 3:4]))[valid]
    return Image.fromarray(np.clip(out * 255 + .5, 0, 255).astype('uint8'))


def font(size):
    for name in ('segoeuib.ttf', 'arialbd.ttf'):
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            pass
    return ImageFont.load_default()


def ring(image, rect, number, crop):
    draw = ImageDraw.Draw(image)
    x, y, w, h = [v * SCALE for v in rect]
    x -= crop[0] * SCALE
    y -= crop[1] * SCALE
    draw.rounded_rectangle([x, y, x + w, y + h], radius=12, outline=RING, width=5)
    if number:
        size = 40
        bx = max(0, min(x - size * .45, image.width - size))
        by = max(0, min(y - size * .45, image.height - size))
        draw.ellipse([bx, by, bx + size, by + size], fill=RING)
        draw.text((bx + size / 2, by + size / 2), str(number), fill=(20, 24, 26, 255), font=font(24), anchor='mm')


def main():
    source, out, pack = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
    only = set(sys.argv[4:])
    out.mkdir(parents=True, exist_ok=True)
    for folder in sorted(p for p in source.iterdir() if (p / 'spec.json').exists()):
        if only and folder.name not in only:
            continue
        spec = json.loads((folder / 'spec.json').read_text())
        crop = spec['crop']
        image = rasterize(folder, crop)
        for r in spec['rings']:
            ring(image, r['rect'], r['number'], crop)
        scale = min(1., 1360 / image.width)
        if scale < 1:
            image = image.resize((round(image.width * scale), round(image.height * scale)), Image.LANCZOS)
        image.convert('RGB').quantize(colors=200, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE).save(out / f'{folder.name}.png', optimize=True)
    with open(pack, 'wb') as f:
        names = sorted(out.glob('*.png'))
        f.write(struct.pack('<I', len(names)))
        for path in names:
            name = path.stem.encode()
            data = path.read_bytes()
            f.write(struct.pack('<HI', len(name), len(data)))
            f.write(name)
            f.write(data)
    print(f'{len(names)} pictures, {pack.stat().st_size / 1e6:.1f} MB')


if __name__ == '__main__':
    main()
