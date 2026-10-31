"""Render egui's exported fixture meshes without a window or desktop capture.
Input is only test-generated meshes.json and its RGBA texture atlas.
"""
import json
import sys
from pathlib import Path
import numpy as np
from PIL import Image


def render(root, width=262, height=980):
    out = np.zeros((height, width, 4), dtype=np.float32)
    out[:] = [56/255, 62/255, 65/255, 1]
    textures = {}
    for mesh in json.loads((root / 'meshes.json').read_text()):
        name = mesh['texture']
        if name not in textures:
            textures[name] = np.asarray(Image.open(root / name).convert('RGBA'), dtype=np.float32) / 255
        tex = textures[name]
        vertices = mesh['vertices']
        clip = mesh['clip']
        for i in range(0, len(mesh['indices']), 3):
            v = [vertices[j] for j in mesh['indices'][i:i+3]]
            p = np.array([a[:2] for a in v], dtype=np.float32)
            x0 = max(0, int(np.floor(p[:, 0].min())), int(np.ceil(clip[0])))
            y0 = max(0, int(np.floor(p[:, 1].min())), int(np.ceil(clip[1])))
            x1 = min(width, int(np.ceil(p[:, 0].max())), int(np.ceil(clip[2])))
            y1 = min(height, int(np.ceil(p[:, 1].max())), int(np.ceil(clip[3])))
            if x0 >= x1 or y0 >= y1:
                continue
            den = (p[1, 1]-p[2, 1])*(p[0, 0]-p[2, 0])+(p[2, 0]-p[1, 0])*(p[0, 1]-p[2, 1])
            if abs(den) < 1e-8:
                continue
            y, x = np.mgrid[y0:y1, x0:x1].astype(np.float32)
            x += .5; y += .5
            b0 = ((p[1, 1]-p[2, 1])*(x-p[2, 0])+(p[2, 0]-p[1, 0])*(y-p[2, 1]))/den
            b1 = ((p[2, 1]-p[0, 1])*(x-p[2, 0])+(p[0, 0]-p[2, 0])*(y-p[2, 1]))/den
            b2 = 1-b0-b1
            valid = (b0 >= 0) & (b1 >= 0) & (b2 >= 0)
            bary = np.stack([b0, b1, b2], axis=-1)
            uv = bary @ np.array([a[2:4] for a in v])
            tx = np.clip(uv[..., 0]*tex.shape[1]-.5, 0, tex.shape[1]-1)
            ty = np.clip(uv[..., 1]*tex.shape[0]-.5, 0, tex.shape[0]-1)
            ix, iy = tx.astype(int), ty.astype(int)
            jx, jy = np.minimum(ix+1, tex.shape[1]-1), np.minimum(iy+1, tex.shape[0]-1)
            fx, fy = (tx-ix)[..., None], (ty-iy)[..., None]
            sample = (tex[iy, ix]*(1-fx)+tex[iy, jx]*fx)*(1-fy)+(tex[jy, ix]*(1-fx)+tex[jy, jx]*fx)*fy
            colors = bary @ (np.array([a[4] for a in v], dtype=np.float32)/255)
            src = sample*colors
            dest = out[y0:y1, x0:x1]
            dest[valid] = (src + dest*(1-src[..., 3:4]))[valid]
    image = Image.fromarray(np.clip(out*255+.5, 0, 255).astype('uint8'))
    image.save(root / 'pane.png')
    return image

if __name__ == '__main__':
    base = Path(sys.argv[1])
    width, height = (int(sys.argv[2]), int(sys.argv[3])) if len(sys.argv) > 3 else (262, 980)
    panes = [render(p,width,height) for p in sorted(base.iterdir()) if (p/'meshes.json').exists()]
    combined = Image.new('RGBA', (width*len(panes), height))
    for n, pane in enumerate(panes): combined.paste(pane, (width*n, 0))
    combined.save(base/'properties-contact-sheet.png')
    print(f'Rendered {len(panes)} headless property panes to {base}')
