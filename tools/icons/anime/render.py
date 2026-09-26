"""Preview renderer replicating bd_shapes voxelize semantics (integer voxel centres, eff half extents,
sequential add/subtract, XYZ euler = Rx*Ry*Rz). Renders a 3/4 view contact sheet."""
import json, math, sys
import numpy as np
from PIL import Image, ImageDraw

def rot(deg):
    x, y, z = [math.radians(d) for d in deg]
    rx = np.array([[1,0,0],[0,math.cos(x),-math.sin(x)],[0,math.sin(x),math.cos(x)]])
    ry = np.array([[math.cos(y),0,math.sin(y)],[0,1,0],[-math.sin(y),0,math.cos(y)]])
    rz = np.array([[math.cos(z),-math.sin(z),0],[math.sin(z),math.cos(z),0],[0,0,1]])
    return rx @ ry @ rz

def eff(h):
    return 0.51 if h < 0.51 else h - 1e-3

def voxelize(prims):
    vox = {}
    for p in prims:
        c = np.array(p['center'], float); s = np.abs(np.array(p['size'], float))
        e = np.array([eff(s[0]/2), eff(s[1]/2), eff(s[2]/2)])
        R = rot(p.get('rot', (0,0,0)))
        corners = np.array([[sx*e[0], sy*e[1], sz*e[2]] for sx in (-1,1) for sy in (-1,1) for sz in (-1,1)])
        w = corners @ R.T + c
        lo = np.ceil(w.min(0) - 1e-4).astype(int); hi = np.floor(w.max(0) + 1e-4).astype(int)
        xs, ys, zs = np.meshgrid(np.arange(lo[0], hi[0]+1), np.arange(lo[1], hi[1]+1), np.arange(lo[2], hi[2]+1), indexing='ij')
        pts = np.stack([xs.ravel(), ys.ravel(), zs.ravel()], 1).astype(float)
        l = (pts - c) @ R  # inverse rotation
        k = p['kind']
        if k == 'Box':
            m = (np.abs(l) <= e).all(1)
        elif k == 'Ellipsoid':
            m = ((l / e) ** 2).sum(1) <= 1
        elif k == 'Cylinder':
            m = (np.abs(l[:,1]) <= e[1]) & ((l[:,0]/e[0])**2 + (l[:,2]/e[2])**2 <= 1)
        else:
            f = (l[:,1] + e[1]) / (2*e[1])
            rx = np.maximum(e[0]*(1-f), 0.45); rz = np.maximum(e[2]*(1-f), 0.45)
            m = (np.abs(l[:,1]) <= e[1]) & ((l[:,0]/rx)**2 + (l[:,2]/rz)**2 <= 1)
        sel = pts[m].astype(int)
        t = p.get('tint', 'Primary')
        for v in map(tuple, sel):
            if p.get('sub'):
                vox.pop(v, None)
            else:
                vox[v] = t
    return vox

PAL = {'Primary': (60, 200, 90), 'Dark': (20, 90, 40), 'Core': (235, 255, 235), 'Accent': (150, 240, 170)}

def render(vox, size, accent=None, yaw=35, pitch=25):
    if not vox:
        return Image.new('RGB', (size, size), (20, 20, 28))
    keys = np.array(list(vox.keys()), float)
    cols = [vox[tuple(int(a) for a in k)] for k in keys]
    # rotate for view: yaw about Y (camera looks from +X,-Z front-ish), pitch about X
    a = math.radians(yaw); b = math.radians(pitch)
    Ry = np.array([[math.cos(a),0,math.sin(a)],[0,1,0],[-math.sin(a),0,math.cos(a)]])
    Rx = np.array([[1,0,0],[0,math.cos(b),-math.sin(b)],[0,math.sin(b),math.cos(b)]])
    # camera looks along +Z in view space; model front is -Z, so we view from -Z side: flip z
    M = Rx @ Ry
    P = keys @ M.T
    lo = P.min(0) - 1; hi = P.max(0) + 1
    ext = max(hi[0]-lo[0], hi[1]-lo[1])
    sc = (size - 16) / ext
    cx = (lo[0]+hi[0])/2; cy = (lo[1]+hi[1])/2
    order = np.argsort(-P[:,2])  # far first (larger z = farther)
    img = Image.new('RGB', (size, size), (20, 20, 28))
    d = ImageDraw.Draw(img)
    r = max(1.0, sc * 0.62)
    pal = dict(PAL)
    if accent:
        pal['Accent'] = tuple(int(c*255) for c in accent)
    zmin, zmax = P[:,2].min(), P[:,2].max()
    for i in order:
        x = size/2 + (P[i,0]-cx)*sc; y = size/2 - (P[i,1]-cy)*sc
        base = np.array(pal[cols[i]], float)
        shade = 1.0 - 0.45 * (P[i,2]-zmin)/max(zmax-zmin, 1e-3)
        c = tuple(int(min(255, v*shade)) for v in base)
        d.rectangle([x-r, y-r, x+r, y+r], fill=c)
    return img

def sheet(entries, out, tile=220, ncol=8):
    n = len(entries); rows = (n + ncol - 1)//ncol
    img = Image.new('RGB', (ncol*tile, rows*(tile+14)), (10,10,14))
    d = ImageDraw.Draw(img)
    for i, e in enumerate(entries):
        vox = voxelize(e['prims'])
        t = render(vox, tile, e.get('accent'), yaw=-60)
        t2 = render(vox, tile//2, e.get('accent'), yaw=-90, pitch=0)
        x = (i % ncol)*tile; y = (i//ncol)*(tile+14)
        img.paste(t, (x, y+14))
        img.paste(t2.resize((tile//3, tile//3)), (x+tile-tile//3, y+14))
        d.text((x+3, y+1), f"{e['id'][:24]} {len(vox)}v", fill=(230,230,230))
    img.save(out)
    print('wrote', out, n)

if __name__ == '__main__':
    data = json.load(open(sys.argv[1]))
    only = sys.argv[3].split(',') if len(sys.argv) > 3 else None
    if only:
        data = [e for e in data if e['id'] in only]
    sheet(data, sys.argv[2])
