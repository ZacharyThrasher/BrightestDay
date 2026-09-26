import { BufferAttribute, BufferGeometry, Color, Mesh, Quaternion, Vector3 } from 'three';
import type { StaticGeom } from '../sim/world.js';
import { staticsMaterial } from './toon.js';

const KIND: Record<StaticGeom['kind'], number> = { ground: 0, building: 1, rock: 2, wall: 3, goal: 4, ring: 4, prop: 4 };

/**
 * Triangle soup builder with barycentric edge flags. Every static primitive is emitted into ONE
 * buffer so the whole city is a single draw call (plus one for its shadow).
 */
export class Soup {
  pos: number[] = [];
  nrm: number[] = [];
  col: number[] = [];
  bary: number[] = [];
  kind: number[] = [];
  private q = new Quaternion();
  private o = new Vector3();
  private c = new Color();
  private k = 0;

  begin(pos: Vector3, rot: Quaternion, color: number, kind: number) {
    this.o.copy(pos);
    this.q.copy(rot);
    this.c.setHex(color);
    this.k = kind;
  }

  /** Triangle in local space; flags = [ab, bc, ca] edge visible. n = outward normal hint per vertex (smooth) or face. */
  tri(a: Vector3, b: Vector3, c: Vector3, flags: [number, number, number], na: Vector3, nb = na, nc = na) {
    // fix winding so the geometric normal agrees with the hint
    const cr = new Vector3().subVectors(b, a).cross(new Vector3().subVectors(c, a));
    if (cr.dot(new Vector3().add(na).add(nb).add(nc)) < 0) {
      [b, c] = [c, b];
      [nb, nc] = [nc, nb];
      flags = [flags[2], flags[1], flags[0]]; // ab<->ac swap: edges become ac, cb, ba
    }
    const verts = [a, b, c];
    const nors = [na, nb, nc];
    // component j of the barycentric belongs to the edge opposite vertex j: bc(0) ca(1) ab(2)
    const edgeOn = [flags[1], flags[2], flags[0]];
    for (let v = 0; v < 3; v++) {
      const p = verts[v].clone().applyQuaternion(this.q).add(this.o);
      const n = nors[v].clone().applyQuaternion(this.q).normalize();
      this.pos.push(p.x, p.y, p.z);
      this.nrm.push(n.x, n.y, n.z);
      this.col.push(this.c.r, this.c.g, this.c.b);
      for (let j = 0; j < 3; j++) this.bary.push(edgeOn[j] ? (j === v ? 1 : 0) : 1);
      this.kind.push(this.k);
    }
  }

  /** Quad a-b-c-d (any winding); flags = [ab, bc, cd, da]; the diagonal a-c is never drawn. */
  quad(a: Vector3, b: Vector3, c: Vector3, d: Vector3, n: Vector3, flags: [number, number, number, number] = [1, 1, 1, 1], n2?: Vector3[]) {
    const [na, nb, nc, nd] = n2 ?? [n, n, n, n];
    this.tri(a, b, c, [flags[0], flags[1], 0], na, nb, nc);
    this.tri(a, c, d, [0, flags[2], flags[3]], na, nc, nd);
  }

  box(s: Vector3) {
    const x = s.x / 2, y = s.y / 2, z = s.z / 2;
    const V = (a: number, b: number, c: number) => new Vector3(a, b, c);
    this.quad(V(-x, -y, z), V(x, -y, z), V(x, y, z), V(-x, y, z), V(0, 0, 1));
    this.quad(V(x, -y, -z), V(-x, -y, -z), V(-x, y, -z), V(x, y, -z), V(0, 0, -1));
    this.quad(V(x, -y, z), V(x, -y, -z), V(x, y, -z), V(x, y, z), V(1, 0, 0));
    this.quad(V(-x, -y, -z), V(-x, -y, z), V(-x, y, z), V(-x, y, -z), V(-1, 0, 0));
    this.quad(V(-x, y, z), V(x, y, z), V(x, y, -z), V(-x, y, -z), V(0, 1, 0));
    this.quad(V(-x, -y, -z), V(x, -y, -z), V(x, -y, z), V(-x, -y, z), V(0, -1, 0));
  }

  wedge(s: Vector3) {
    const x = s.x / 2, y = s.y / 2, z = s.z / 2;
    const V = (a: number, b: number, c: number) => new Vector3(a, b, c);
    this.quad(V(-x, -y, -z), V(x, -y, -z), V(x, -y, z), V(-x, -y, z), V(0, -1, 0));
    this.quad(V(-x, -y, -z), V(x, -y, -z), V(x, y, z), V(-x, y, z), new Vector3(0, s.z, -s.y).normalize());
    this.quad(V(-x, -y, z), V(x, -y, z), V(x, y, z), V(-x, y, z), V(0, 0, 1));
    this.tri(V(-x, -y, -z), V(-x, -y, z), V(-x, y, z), [1, 1, 1], V(-1, 0, 0));
    this.tri(V(x, -y, -z), V(x, -y, z), V(x, y, z), [1, 1, 1], V(1, 0, 0));
  }

  cylinder(r: number, h: number, seg = 18) {
    const y = h / 2;
    for (let i = 0; i < seg; i++) {
      const a0 = (i / seg) * Math.PI * 2, a1 = ((i + 1) / seg) * Math.PI * 2;
      const n0 = new Vector3(Math.cos(a0), 0, Math.sin(a0)), n1 = new Vector3(Math.cos(a1), 0, Math.sin(a1));
      const b0 = n0.clone().multiplyScalar(r).setY(-y), b1 = n1.clone().multiplyScalar(r).setY(-y);
      const t0 = b0.clone().setY(y), t1 = b1.clone().setY(y);
      this.quad(b0, b1, t1, t0, n0, [1, 0, 1, 0], [n0, n1, n1, n0]);
      this.tri(new Vector3(0, y, 0), t0, t1, [0, 1, 0], new Vector3(0, 1, 0));
      this.tri(new Vector3(0, -y, 0), b1, b0, [0, 1, 0], new Vector3(0, -1, 0));
    }
  }

  geometry(): BufferGeometry {
    const g = new BufferGeometry();
    g.setAttribute('position', new BufferAttribute(new Float32Array(this.pos), 3));
    g.setAttribute('normal', new BufferAttribute(new Float32Array(this.nrm), 3));
    g.setAttribute('color', new BufferAttribute(new Float32Array(this.col), 3));
    g.setAttribute('aBary', new BufferAttribute(new Float32Array(this.bary), 3));
    g.setAttribute('aKind', new BufferAttribute(new Float32Array(this.kind), 1));
    g.computeBoundingSphere();
    return g;
  }
}

/** Merge all static geometry of a world into one toon mesh (1 draw call). */
export function buildStatics(statics: StaticGeom[]): Mesh {
  const soup = new Soup();
  for (const s of statics) {
    soup.begin(s.pos, s.rot, s.color, KIND[s.kind] ?? 4);
    if (s.shape === 'box') soup.box(s.size);
    else if (s.shape === 'wedge') soup.wedge(s.size);
    else soup.cylinder(s.size.x, s.size.y);
  }
  const mesh = new Mesh(soup.geometry(), staticsMaterial());
  mesh.castShadow = true;
  mesh.receiveShadow = true;
  mesh.name = 'statics';
  mesh.matrixAutoUpdate = false;
  return mesh;
}
