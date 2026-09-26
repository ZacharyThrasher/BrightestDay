import { BackSide, Color, FrontSide, Group, LineSegments, Mesh, Quaternion, Vector3, type ShaderMaterial } from 'three';
import type { World } from '../sim/world.js';
import type { Construct } from '../sim/constructs/runtime.js';
import type { ConstructSpec } from '../sim/constructs/spec.js';
import { cloneSpec } from '../sim/constructs/spec.js';
import { specExtent } from '../sim/constructs/geom.js';
import {
  buildConstructGeometry, edgeMaterial, fillMaterial, ghostMaterial, hardlightUniforms, shareInstanceAttributes,
  type ConstructGeometry, type HardlightUniforms,
} from './hardlight.js';
import { tintFor } from './config.js';
import type { FX } from './fx.js';

const FORM_TIME = 0.34; // seconds for a fresh construct to assemble
const REFORM_TIME = 0.22;
const DISSOLVE_TIME = 0.5;

interface View {
  id: string;
  version: number;
  count: number;
  geo: ConstructGeometry;
  u: HardlightUniforms;
  group: Group;
  mats: ShaderMaterial[];
  prev: Float32Array; // per inst: pos(3) + quat(4)
  curr: Float32Array;
  formT: number; // seconds since (re)formation began
  formDur: number;
  formStart: number; // uForm start value (reforms start part-way)
  dissolveT: number; // <0 = alive
  color: Color;
  label: string;
  flash: number;
  maxDelay: number;
}

interface Ghost {
  key: string;
  spec: ConstructSpec;
  geo: ConstructGeometry;
  u: HardlightUniforms;
  group: Group;
  mats: ShaderMaterial[];
  count: number;
}

const _p = new Vector3(), _q = new Quaternion(), _q2 = new Quaternion(), _v = new Vector3();

/** Hard-light construct visuals: assembly, interpolation, reform flash, dissolve and ghost previews. */
export class ConstructViews {
  readonly root = new Group();
  private views = new Map<string, View>();
  private dying: View[] = [];
  private ghosts: Ghost[] = [];
  private ghostSpecs: ConstructSpec[] = [];
  private baseColor: Color;
  focusId: string | null = null;

  constructor(color: string, private fx: FX) {
    this.baseColor = new Color(color);
    this.root.name = 'constructs';
  }

  setColor(color: string) { this.baseColor.set(color); }

  get drawCount() { return this.views.size + this.dying.length; }

  /** Called after every sim tick. */
  tick(w: World, handPos: Vector3) {
    this.focusId = w.constructs.focusId;
    for (const c of w.constructs.drainRemoved()) {
      const v = this.views.get(c.id);
      if (!v) continue;
      this.views.delete(c.id);
      v.dissolveT = 0;
      this.dying.push(v);
      // shed particles from each instance
      for (let k = 0; k < v.count; k++) {
        _p.fromArray(v.curr, k * 7);
        for (const ctr of v.geo.centers) {
          _q.fromArray(v.curr, k * 7 + 3);
          const at = ctr.clone().applyQuaternion(_q).add(_p);
          this.fx.glow(at, v.color, 0.8 + Math.random() * 0.6, 0.45, 1.6);
          this.fx.sparks(at, v.color, 3, 5, { life: 0.6, size: 0.05 });
        }
      }
    }
    for (const c of w.constructs.list.values()) {
      let v = this.views.get(c.id);
      const n = c.insts.length;
      if (!v || v.version !== c.version || v.count !== n) {
        const fresh = !v;
        if (v) this.disposeView(v);
        v = this.buildView(c, handPos, fresh);
        this.views.set(c.id, v);
        if (!fresh) { v.formStart = 0.45; v.formDur = REFORM_TIME; v.flash = 1; }
      } else {
        v.prev.set(v.curr);
      }
      for (let k = 0; k < n; k++) {
        const inst = c.insts[k];
        inst.pos.toArray(v.curr, k * 7);
        inst.quat.toArray(v.curr, k * 7 + 3);
      }
      if (v.formT === 0) v.prev.set(v.curr);
    }
  }

  private buildView(c: Construct, handPos: Vector3, fresh: boolean): View {
    const n = Math.max(1, c.insts.length);
    const inst0 = c.insts[0];
    const local = inst0 ? handPos.clone().sub(inst0.pos).applyQuaternion(inst0.quat.clone().invert()) : null;
    const geo = buildConstructGeometry(c.spec, fresh ? local : null, n);
    shareInstanceAttributes(geo.fill, geo.edges);
    const color = new Color(tintFor(c.spec.color, '#' + this.baseColor.getHexString()));
    const u = hardlightUniforms(color);
    u.uShell.value = c.spec.parts.some((p) => p.role === 'shell' || p.shape === 'dome') ? 1 : 0;
    const back = fillMaterial(u, BackSide);
    const front = fillMaterial(u, FrontSide);
    const edge = edgeMaterial(u);
    const group = new Group();
    const mBack = new Mesh(geo.fill, back); mBack.frustumCulled = false; mBack.renderOrder = 10;
    const mFront = new Mesh(geo.fill, front); mFront.frustumCulled = false; mFront.renderOrder = 11;
    const mEdge = new LineSegments(geo.edges, edge); mEdge.frustumCulled = false; mEdge.renderOrder = 12;
    group.add(mBack, mFront, mEdge);
    this.root.add(group);
    const delays = geo.fill.getAttribute('iDelay').array as Float32Array;
    for (let k = 0; k < n; k++) delays[k] = n > 1 ? (k / (n - 1)) * 0.5 : 0;
    geo.fill.getAttribute('iDelay').needsUpdate = true;
    return {
      id: c.id, version: c.version, count: c.insts.length, geo, u, group, mats: [back, front, edge], prev: new Float32Array(n * 7), curr: new Float32Array(n * 7),
      formT: 0, formDur: FORM_TIME, formStart: 0, dissolveT: -1, color, label: c.spec.label, flash: 0, maxDelay: n > 1 ? 0.5 : 0,
    };
  }

  private disposeView(v: View) {
    this.root.remove(v.group);
    v.geo.fill.dispose();
    v.geo.edges.dispose();
    v.mats.forEach((m) => m.dispose());
  }

  /** Per render frame: interpolate poses, animate assembly/dissolve, draw forming beams. */
  frame(alpha: number, dt: number, time: number, handPos: Vector3) {
    for (const v of this.views.values()) {
      v.formT += dt;
      const fp = Math.min(1, v.formT / v.formDur);
      v.u.uForm.value = v.formStart + (1 + v.maxDelay - v.formStart) * fp;
      v.flash = Math.max(0, v.flash - dt * 3);
      v.u.uFlash.value = v.flash + (fp < 1 ? (1 - fp) * 0.6 : 0);
      v.u.uTime.value = time;
      this.writeInstances(v, alpha);
      // assembly beam from the ring hand
      if (fp < 1 && v.formStart === 0) {
        for (let k = 0; k < v.count; k++) {
          _p.fromArray(v.curr, k * 7);
          const s = 1 - fp;
          this.fx.beams.push(handPos, _p, 0.05 * s + 0.02, 0.25 * s + 0.05, v.color, 2.5 * s + 0.3);
        }
      }
    }
    for (let i = this.dying.length - 1; i >= 0; i--) {
      const v = this.dying[i];
      v.dissolveT += dt;
      v.u.uDissolve.value = Math.min(1, v.dissolveT / DISSOLVE_TIME);
      v.u.uTime.value = time;
      v.u.uOpacity.value = 1 - Math.max(0, (v.dissolveT / DISSOLVE_TIME - 0.6) / 0.4);
      this.writeInstances(v, 1);
      if (v.dissolveT >= DISSOLVE_TIME) { this.disposeView(v); this.dying.splice(i, 1); }
    }
  }

  private writeInstances(v: View, alpha: number) {
    const P = v.geo.fill.getAttribute('iPos');
    const Q = v.geo.fill.getAttribute('iQuat');
    const pa = P.array as Float32Array, qa = Q.array as Float32Array;
    for (let k = 0; k < v.count; k++) {
      _p.fromArray(v.prev, k * 7).lerp(_v.fromArray(v.curr, k * 7), alpha);
      _q.fromArray(v.prev, k * 7 + 3).slerp(_q2.fromArray(v.curr, k * 7 + 3), alpha);
      pa[k * 3] = _p.x; pa[k * 3 + 1] = _p.y; pa[k * 3 + 2] = _p.z;
      qa[k * 4] = _q.x; qa[k * 4 + 1] = _q.y; qa[k * 4 + 2] = _q.z; qa[k * 4 + 3] = _q.w;
    }
    v.geo.fill.instanceCount = v.count;
    v.geo.edges.instanceCount = v.count;
    P.needsUpdate = true;
    Q.needsUpdate = true;
  }

  /** Interpolated world position of a construct's first instance (HUD / effects). */
  positionOf(id: string, out = new Vector3()): Vector3 | null {
    const v = this.views.get(id);
    if (!v) return null;
    const P = v.geo.fill.getAttribute('iPos');
    return out.set(P.getX(0), P.getY(0), P.getZ(0));
  }

  // ---------------------------------------------------------------------------------------------
  // Ghost previews (while typing/speaking)
  // ---------------------------------------------------------------------------------------------

  setPreview(specs: ConstructSpec[]) { this.ghostSpecs = specs.map(cloneSpec); }

  frameGhosts(w: World, time: number) {
    const want: { spec: ConstructSpec; pos: Vector3; quat: Quaternion; offs: Vector3[] }[] = [];
    for (const s of this.ghostSpecs) {
      const spec = cloneSpec(s);
      let pose: { pos: Vector3; quat: Quaternion };
      try { pose = w.constructs.place(spec); } catch { continue; }
      const ext = specExtent(spec);
      let offs: Vector3[] = [new Vector3()];
      try {
        const f = (w.constructs as any).arrangementOffsets;
        if (typeof f === 'function' && spec.count > 1) offs = f.call(w.constructs, spec, ext);
      } catch { /* private API may change; fall back to one ghost */ }
      want.push({ spec, pos: pose.pos, quat: pose.quat, offs });
    }
    // rebuild ghost meshes when shape changes (key excludes pose)
    for (let i = 0; i < Math.max(want.length, this.ghosts.length); i++) {
      const w1 = want[i];
      const g = this.ghosts[i];
      if (!w1) { if (g) this.disposeGhost(g); continue; }
      const st = w1.spec.stretch?.map((x) => Math.round(x * 20) / 20);
      const key = JSON.stringify([w1.spec.parts, w1.spec.scale, st, w1.offs.length, w1.spec.color]);
      if (!g || g.key !== key) {
        if (g) this.disposeGhost(g);
        this.ghosts[i] = this.buildGhost(w1.spec, key, w1.offs.length);
      }
    }
    this.ghosts.length = want.length;
    for (let i = 0; i < want.length; i++) {
      const g = this.ghosts[i];
      const wt = want[i];
      g.u.uTime.value = time;
      const P = g.geo.fill.getAttribute('iPos');
      const Q = g.geo.fill.getAttribute('iQuat');
      for (let k = 0; k < g.count; k++) {
        const p = wt.offs[k].clone().applyQuaternion(wt.quat).add(wt.pos);
        P.setXYZ(k, p.x, p.y, p.z);
        Q.setXYZW(k, wt.quat.x, wt.quat.y, wt.quat.z, wt.quat.w);
      }
      P.needsUpdate = true;
      Q.needsUpdate = true;
    }
  }

  private buildGhost(spec: ConstructSpec, key: string, count: number): Ghost {
    const geo = buildConstructGeometry(spec, null, count);
    shareInstanceAttributes(geo.fill, geo.edges);
    const color = new Color(tintFor(spec.color, '#' + this.baseColor.getHexString())).lerp(new Color(1, 1, 1), 0.25);
    const u = hardlightUniforms(color);
    u.uGhost.value = 1;
    u.uOpacity.value = 0.9;
    const fill = ghostMaterial(u);
    const edge = edgeMaterial(u);
    const group = new Group();
    const m = new Mesh(geo.fill, fill); m.frustumCulled = false; m.renderOrder = 13;
    const e = new LineSegments(geo.edges, edge); e.frustumCulled = false; e.renderOrder = 14;
    group.add(m, e);
    this.root.add(group);
    return { key, spec, geo, u, group, mats: [fill, edge], count };
  }

  private disposeGhost(g: Ghost) {
    this.root.remove(g.group);
    g.geo.fill.dispose();
    g.geo.edges.dispose();
    g.mats.forEach((m) => m.dispose());
  }

  get ghostCount() { return this.ghosts.length; }

  clear() {
    for (const v of this.views.values()) this.disposeView(v);
    for (const v of this.dying) this.disposeView(v);
    for (const g of this.ghosts) this.disposeGhost(g);
    this.views.clear();
    this.dying = [];
    this.ghosts = [];
  }
}
