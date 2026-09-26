"""Shape builders for anime icons. Units = construct voxels (0.25 m); voxel centres on the integer
lattice. Held: grip at origin, points -Z, strike face -Y. Creatures: feet centre, face -Z, left = -X.
Worn: origin upper back, gear at +Z. Structures: base centre, front -Z. Objects: centre, fwd -Z."""
import math

T = {'P': 'Primary', 'A': 'Accent', 'D': 'Dark', 'C': 'Core'}
ALONG_Z = (90, 0, 0)
ALONG_X = (0, 0, 90)
TIP_NZ = (-90, 0, 0)   # cone tip toward -Z
TIP_PZ = (90, 0, 0)    # cone tip toward +Z
TIP_NY = (180, 0, 0)
TIP_PX = (0, 0, -90)
TIP_NX = (0, 0, 90)


class S:
    def __init__(self):
        self.prims = []
        self.parts = []

    def add(self, kind, c, s, t='P', r=(0, 0, 0), part=0, sub=False):
        self.prims.append(dict(kind=kind, center=tuple(float(v) for v in c), size=tuple(float(v) for v in s),
                               rot=tuple(float(v) for v in r), tint=T[t], part=part, sub=sub))
        return self

    def box(self, c, s, t='P', r=(0, 0, 0), part=0, sub=False):
        return self.add('Box', c, s, t, r, part, sub)

    def ell(self, c, s, t='P', r=(0, 0, 0), part=0, sub=False):
        return self.add('Ellipsoid', c, s, t, r, part, sub)

    def cyl(self, c, s, t='P', r=(0, 0, 0), part=0, sub=False):
        return self.add('Cylinder', c, s, t, r, part, sub)

    def cone(self, c, s, t='P', r=(0, 0, 0), part=0, sub=False):
        return self.add('Cone', c, s, t, r, part, sub)

    def vb(self, x0, x1, y0, y1, z0, z1, t='P', part=0, sub=False):
        """Axis box covering integer voxel ranges inclusive."""
        x0, x1 = min(x0, x1), max(x0, x1); y0, y1 = min(y0, y1), max(y0, y1); z0, z1 = min(z0, z1), max(z0, z1)
        return self.box(((x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2), (x1 - x0 + 1, y1 - y0 + 1, z1 - z0 + 1), t, (0, 0, 0), part, sub)

    def part(self, id, parent, pivot, role):
        self.parts.append(dict(id=id, parent=parent, pivot=tuple(float(v) for v in pivot), role=role))
        return self

    def seg(self, a, b, w, h, t='P', part=0, kind='Box'):
        """Prim whose local Y runs from a to b (w = local x size, h = local z size)."""
        ax, ay, az = a; bx, by, bz = b
        dx, dy, dz = bx - ax, by - ay, bz - az
        L = math.sqrt(dx * dx + dy * dy + dz * dz)
        horiz = math.sqrt(dx * dx + dz * dz)
        tilt = math.degrees(math.atan2(horiz, dy))
        yaw = math.degrees(math.atan2(-dz, dx)) if horiz > 1e-6 else 0.0
        c = ((ax + bx) / 2, (ay + by) / 2, (az + bz) / 2)
        return self.add(kind, c, (w, max(L, 1), h), t, (0, yaw, -tilt), part)

    def spec(self):
        return dict(prims=self.prims, parts=self.parts)


# ------------------------------------------------------------------------------------------ held

def katana(blade_t='P', edge='C', guard='A', grip='D', L=7, reverse=False, guard_kind='round', h=2):
    s = S()
    s.vb(0, 0, 0, 0, -1, 2, grip)                 # grip (hand at 0)
    s.vb(0, 0, 0, 0, 3, 3, guard)                  # pommel cap
    if guard_kind == 'round':
        s.cyl((0, 0, -2), (3, 1, 3), guard, ALONG_Z)
    elif guard_kind == 'cross':
        s.vb(0, 0, -2, 2, -2, -2, guard)
        s.vb(-1, 1, 0, 0, -2, -2, guard)
    elif guard_kind == 'none':
        s.vb(0, 0, 0, 0, -2, -2, 'D')
    z0, z1 = -3, -3 - L + 1
    s.vb(0, 0, -h + 1, 0, z1, z0, blade_t)
    ey = 0 if reverse else -h + 1
    s.vb(0, 0, ey, ey, z1, z0, edge)
    s.vb(0, 0, ey, ey, z1 - 1, z1 - 1, edge)  # tip
    return s


def fist(s, z, w=3, h=3, d=3, t='P', part=0, knuckle='A'):
    """Fist block centred at z (front face toward -Z), knuckle row on top-front, thumb on -X."""
    s.box((0, 0, z), (w, h, d), t, part=part)
    s.box((0, (h - 1) / 2, z - (d - 1) / 2), (w, 1, 1), knuckle, part=part)
    s.box((-(w + 1) / 2, 0, z), (1, max(1, h - 2), max(1, d - 1)), t, part=part)
    return s


# ------------------------------------------------------------------------------------ creatures

def humanoid(s, leg_h=4, leg_w=1, hip_x=1, torso=(5, 4, 3), arm_w=1, arm_len=4, head=(3, 3, 3),
             tl='P', tt='P', ta='P', th='P', head_kind='box', shoulder_extra=0, leg_d=None, arm_d=None):
    """Parts: 1 legL, 2 legR, 3 armL, 4 armR, 5 head. Returns key heights."""
    tw, tH, td = torso
    leg_d = leg_d or leg_w
    arm_d = arm_d or arm_w
    y_hip = leg_h
    y_sh = leg_h + tH - 1
    for pid, sx, role in ((1, -1, 'LegBackLeft'), (2, 1, 'LegBackRight')):
        x = sx * hip_x
        s.box((x, (leg_h - 1) / 2, 0), (leg_w, leg_h, leg_d), tl, part=pid)
        s.part(pid, 0, (x, y_hip - 0.5, 0), role)
    s.vb(-(tw - 1) // 2, (tw - 1) // 2, y_hip, y_sh, -(td - 1) // 2, (td - 1) // 2, tt)
    ax = (tw - 1) // 2 + 1 + (arm_w - 1) / 2 + shoulder_extra
    for pid, sx, role in ((3, -1, 'ArmLeft'), (4, 1, 'ArmRight')):
        x = sx * ax
        s.box((x, y_sh - (arm_len - 1) / 2, 0), (arm_w, arm_len, arm_d), ta, part=pid)
        s.part(pid, 0, (x, y_sh, 0), role)
    hw, hh, hd = head
    hy = y_sh + 1 + (hh - 1) / 2
    if head_kind == 'box':
        s.box((0, hy, 0), (hw, hh, hd), th, part=5)
    else:
        s.ell((0, hy, 0), (hw, hh, hd), th, part=5)
    s.part(5, 0, (0, y_sh + 0.5, 0), 'Head')
    return dict(y_hip=y_hip, y_sh=y_sh, hy=hy, ax=ax, top=y_sh + hh, hd=hd, hw=hw, hh=hh)


def tail_chain(s, start_id, parent, pts, sizes, t='P', kind='Ellipsoid', role='Tail'):
    """Chain of parts through pts; part i spans pts[i]..pts[i+1], pivot at pts[i]."""
    pid = parent
    ids = []
    for i in range(len(pts) - 1):
        a, b = pts[i], pts[i + 1]
        nid = start_id + i
        s.part(nid, pid, a, role)
        sz = sizes[i]
        if kind == 'seg':
            s.seg(a, b, sz[0], sz[1], t, part=nid)
        else:
            s.add(kind, ((a[0] + b[0]) / 2, (a[1] + b[1]) / 2, (a[2] + b[2]) / 2), sz, t, part=nid)
        pid = nid
        ids.append(nid)
    return ids


def octagon_fill(s, c, r, depth, t):
    a = r * math.tan(math.radians(22.5))
    for ang in (0, 45, 90, 135):
        s.box(c, (2 * r, 2 * a + 1, depth), t, (0, 0, ang))


def octagon_ring(s, c, r, depth, t):
    a = r * math.tan(math.radians(22.5))
    for k in range(8):
        ang = k * 45
        x = c[0] + r * math.cos(math.radians(ang)); y = c[1] + r * math.sin(math.radians(ang))
        s.box((x, y, c[2]), (1, 2 * a + 1.2, depth), t, (0, 0, ang))
