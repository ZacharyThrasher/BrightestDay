import { BackSide, Color, FrontSide, Group, LineSegments, Mesh, Quaternion, Vector3, type ShaderMaterial } from 'three';
import type { GameEvent } from '../sim/world.js';
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

/** One articulated part group of a construct (null = the rigid body). Drawn with the construct's shared materials. */
export interface GroupView {
  name: string | null;
  geo: ConstructGeometry;
  prevJ: Float32Array; // per inst: joint pose pos(3) + quat(4) in the construct frame
  currJ: Float32Array;
}

export interface View {
  id: string;
  version: number;
  count: number;
  geo: ConstructGeometry; // the first group's geometry
  groups: GroupView[];
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
  pos0: Vector3; // interpolated position of the first instance (last written)
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

const _p = new Vector3(), _q = new Quaternion(), _q2 = new Quaternion(), _v = new Vector3(), _jp = new Vector3(), _jq = new Quaternion();

interface MachineBeam { from: Vector3; to: Vector3; kind: string; t: number; dur: number; id: string }

/** Hard-light construct visuals: assembly, interpolation, reform flash, dissolve and ghost previews. */
export class ConstructViews {
  readonly root = new Group();
  private views = new Map<string, View>();
  private dying: View[] = [];
  private ghosts: Ghost[] = [];
  private ghostSpecs: ConstructSpec[] = [];
  private baseColor: Color;
  private beams: MachineBeam[] = [];
  focusId: string | null = null;
  /** Constructs of anyone but this player are drawn in `rivalColor` (unless the spec names a colour). */
  localOwner = 'p1';
  rivalColor = new Color('#ff5fb0');
  /** JUICE hook: called after each view's instances are written (hitstop freeze, snap / flash uniforms). */
  poseHook: ((v: View) => void) | null = null;

  constructor(color: string, private fx: FX) {
    this.baseColor = new Color(color);
    this.root.name = 'constructs';
  }

  setColor(color: string) { this.baseColor.set(color); }

  get drawCount() { return this.views.size + this.dying.length; }

  /** Machine weapons (lasers, flame): drawn for a moment with the existing beam/particle fx. */
  onMachineEvent(e: GameEvent) {
    if (e.type !== 'machine_beam' || !e.from || !e.to) return;
    this.beams.push({ from: new Vector3(...(e.from as [number, number, number])), to: new Vector3(...(e.to as [number, number, number])), kind: e.kind ?? 'laser', t: 0, dur: Math.max(0.06, e.dur ?? 0.12), id: e.construct });
    if (this.beams.length > 64) this.beams.splice(0, this.beams.length - 64);
  }

  /** Called after every sim tick. */
  tick(w: World, handPos: Vector3) {
    this.focusId = w.constructs.focusOf(this.localOwner);
    for (const c of w.constructs.drainRemoved()) {
      const v = this.views.get(c.id);
      if (!v) continue;
      this.views.delete(c.id);
      v.dissolveT = 0;
      this.dying.push(v);
      // shed particles from each instance
      for (let k = 0; k < v.count; k++) {
        _p.fromArray(v.curr, k * 7);
        _q.fromArray(v.curr, k * 7 + 3);
        for (const g of v.groups) {
          _jp.fromArray(g.currJ, k * 7);
          _jq.fromArray(g.currJ, k * 7 + 3);
          for (const ctr of g.geo.centers) {
            const at = ctr.clone().applyQuaternion(_jq).add(_jp).applyQuaternion(_q).add(_p);
            this.fx.glow(at, v.color, 0.8 + Math.random() * 0.6, 0.45, 1.6);
            this.fx.sparks(at, v.color, 3, 5, { life: 0.6, size: 0.05 });
          }
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
        for (const g of v.groups) g.prevJ.set(g.currJ);
      }
      for (let k = 0; k < n; k++) {
        const inst = c.insts[k];
        inst.pos.toArray(v.curr, k * 7);
        inst.quat.toArray(v.curr, k * 7 + 3);
        for (const g of v.groups) {
          const pose = g.name !== null ? inst.jointPose?.get(g.name) : undefined;
          if (pose) { pose.pos.toArray(g.currJ, k * 7); pose.quat.toArray(g.currJ, k * 7 + 3); }
          else { g.currJ.fill(0, k * 7, k * 7 + 6); g.currJ[k * 7 + 6] = 1; }
        }
      }
      if (v.formT === 0) { v.prev.set(v.curr); for (const g of v.groups) g.prevJ.set(g.currJ); }
    }
  }

  private buildView(c: Construct, handPos: Vector3, fresh: boolean): View {
    const n = Math.max(1, c.insts.length);
    const inst0 = c.insts[0];
    const local = inst0 ? handPos.clone().sub(inst0.pos).applyQuaternion(inst0.quat.clone().invert()) : null;
    // one geometry per articulated group (parts whose group has a joint); everything else is the rigid body
    const jointGroups = new Set((c.spec.joints ?? []).map((j) => j.group));
    const groupOf = (i: number) => { const g = c.spec.parts[i]?.group; return g && jointGroups.has(g) ? g : null; };
    const names: (string | null)[] = [null, ...(c.spec.joints ?? []).map((j) => j.group).filter((g) => c.spec.parts.some((p) => p.group === g))];
    const own = c.owner === this.localOwner ? this.baseColor : this.rivalColor;
    const color = new Color(tintFor(c.spec.color, '#' + own.getHexString()));
    const u = hardlightUniforms(color);
    u.uShell.value = c.spec.parts.some((p) => p.role === 'shell' || p.shape === 'dome') ? 1 : 0;
    const back = fillMaterial(u, BackSide);
    const front = fillMaterial(u, FrontSide);
    const edge = edgeMaterial(u);
    const group = new Group();
    const groups: GroupView[] = [];
    for (const name of names) {
      if (name === null && names.length > 1 && !c.spec.parts.some((_, i) => groupOf(i) === null)) continue;
      const geo = buildConstructGeometry(c.spec, fresh ? local : null, n, names.length > 1 ? (i) => groupOf(i) === name : undefined);
      shareInstanceAttributes(geo.fill, geo.edges);
      const mBack = new Mesh(geo.fill, back); mBack.frustumCulled = false; mBack.renderOrder = 10;
      const mFront = new Mesh(geo.fill, front); mFront.frustumCulled = false; mFront.renderOrder = 11;
      const mEdge = new LineSegments(geo.edges, edge); mEdge.frustumCulled = false; mEdge.renderOrder = 12;
      group.add(mBack, mFront, mEdge);
      const delays = geo.fill.getAttribute('iDelay').array as Float32Array;
      for (let k = 0; k < n; k++) delays[k] = n > 1 ? (k / (n - 1)) * 0.5 : 0;
      geo.fill.getAttribute('iDelay').needsUpdate = true;
      const ident = new Float32Array(n * 7);
      for (let k = 0; k < n; k++) ident[k * 7 + 6] = 1;
      groups.push({ name, geo, prevJ: ident.slice(), currJ: ident });
    }
    this.root.add(group);
    const geo = groups[0].geo;
    return {
      id: c.id, version: c.version, count: c.insts.length, geo, groups, u, group, mats: [back, front, edge], prev: new Float32Array(n * 7), curr: new Float32Array(n * 7),
      formT: 0, formDur: FORM_TIME, formStart: 0, dissolveT: -1, color, label: c.spec.label, flash: 0, maxDelay: n > 1 ? 0.5 : 0, pos0: new Vector3(),
    };
  }

  private disposeView(v: View) {
    this.root.remove(v.group);
    for (const g of v.groups) { g.geo.fill.dispose(); g.geo.edges.dispose(); }
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
    // machine beams (lasers / flame)
    for (let i = this.beams.length - 1; i >= 0; i--) {
      const b = this.beams[i];
      const first = b.t === 0;
      b.t += dt;
      const v = this.views.get(b.id);
      const col = v?.color ?? this.baseColor;
      const k = 1 - b.t / b.dur;
      if (k <= 0) { this.beams.splice(i, 1); continue; }
      if (b.kind === 'flame') {
        const warm = col.clone().lerp(new Color('#ffb04d'), 0.55);
        const dir = b.to.clone().sub(b.from);
        if (first) for (let n = 0; n < 6; n++) {
          const vel = dir.clone().multiplyScalar(2.2 + Math.random()).add(new Vector3(Math.random() - 0.5, Math.random() - 0.3, Math.random() - 0.5).multiplyScalar(3));
          this.fx.particles.spawn({ pos: b.from.clone(), vel, color: warm, intensity: 2.4, size: 0.35, sizeEnd: 1.4, life: 0.4, drag: 1.5, mode: 0 });
        }
        this.fx.beams.push(b.from, b.to, 0.25 * k, 1.2 * k, warm, 1.4 * k);
      } else {
        this.fx.beams.push(b.from, b.to, 0.09 * k + 0.02, 0.06 * k + 0.02, col, 3.2 * k + 0.4);
        if (first) this.fx.sparks(b.to, col, 6, 7, { size: 0.05, life: 0.3 });
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

  /** Per group: world transform = interpolated instance pose ∘ interpolated joint pose. */
  private writeInstances(v: View, alpha: number) {
    for (let k = 0; k < v.count; k++) {
      _p.fromArray(v.prev, k * 7).lerp(_v.fromArray(v.curr, k * 7), alpha);
      _q.fromArray(v.prev, k * 7 + 3).slerp(_q2.fromArray(v.curr, k * 7 + 3), alpha);
      if (k === 0) v.pos0.copy(_p);
      for (const g of v.groups) {
        const pa = g.geo.fill.getAttribute('iPos').array as Float32Array;
        const qa = g.geo.fill.getAttribute('iQuat').array as Float32Array;
        let wx = _p.x, wy = _p.y, wz = _p.z;
        let q = _q;
        if (g.name !== null) {
          _jp.fromArray(g.prevJ, k * 7).lerp(_v.fromArray(g.currJ, k * 7), alpha);
          _jq.fromArray(g.prevJ, k * 7 + 3).slerp(_q2.fromArray(g.currJ, k * 7 + 3), alpha);
          _jp.applyQuaternion(_q);
          wx += _jp.x; wy += _jp.y; wz += _jp.z;
          q = _jq.premultiply(_q);
        }
        pa[k * 3] = wx; pa[k * 3 + 1] = wy; pa[k * 3 + 2] = wz;
        qa[k * 4] = q.x; qa[k * 4 + 1] = q.y; qa[k * 4 + 2] = q.z; qa[k * 4 + 3] = q.w;
      }
    }
    for (const g of v.groups) {
      g.geo.fill.instanceCount = v.count;
      g.geo.edges.instanceCount = v.count;
      g.geo.fill.getAttribute('iPos').needsUpdate = true;
      g.geo.fill.getAttribute('iQuat').needsUpdate = true;
    }
    this.poseHook?.(v); // JUICE hook
  }

  /** Interpolated world position of a construct's first instance (HUD / effects). */
  positionOf(id: string, out = new Vector3()): Vector3 | null {
    const v = this.views.get(id);
    if (!v) return null;
    return out.copy(v.pos0);
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
      const offs: Vector3[] = spec.count > 1 ? w.constructs.arrangementOffsets(spec, ext) : [new Vector3()];
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
