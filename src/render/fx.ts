import {
  AddEquation, BufferAttribute, BufferGeometry, Color, CustomBlending, DynamicDrawUsage, InstancedBufferAttribute,
  InstancedBufferGeometry, Mesh, OneFactor, OneMinusSrcAlphaFactor, Points, ShaderMaterial, SphereGeometry, Vector3,
  ZeroFactor, type Scene,
} from 'three';

/** Additive colour, alpha channel attenuated so post fog/ink treat bright FX as energy. */
function additive(m: ShaderMaterial) {
  m.transparent = true;
  m.depthWrite = false;
  m.blending = CustomBlending;
  m.blendEquation = AddEquation;
  m.blendSrc = OneFactor;
  m.blendDst = OneFactor;
  m.blendSrcAlpha = ZeroFactor;
  m.blendDstAlpha = OneMinusSrcAlphaFactor;
  return m;
}

// ------------------------------------------------------------------------------------------------
// Particles (point sprites): sparks, glows, shockwave rings, dust. One draw call.
// ------------------------------------------------------------------------------------------------

export interface ParticleOpts {
  pos: Vector3;
  vel?: Vector3;
  color: Color;
  intensity?: number;
  size: number;
  sizeEnd?: number;
  life: number;
  drag?: number;
  gravity?: number;
  mode?: 0 | 1 | 2; // 0 soft glow, 1 ring, 2 hard dot
  streak?: number; // >0: rendered as a velocity-aligned streak of this length factor instead of a point
}

const MAXP = 4096;

class Particles {
  readonly points: Points;
  private pos = new Float32Array(MAXP * 3);
  private vel = new Float32Array(MAXP * 3);
  private col = new Float32Array(MAXP * 4);
  private base = new Float32Array(MAXP * 4); // rgb, intensity
  private size = new Float32Array(MAXP * 2); // start, end
  private life = new Float32Array(MAXP * 2); // age, max
  private phys = new Float32Array(MAXP * 2); // drag, gravity
  private mode = new Float32Array(MAXP);
  private streak = new Float32Array(MAXP);
  private alive = new Uint8Array(MAXP);
  private gPos: BufferAttribute;
  private gCol: BufferAttribute;
  private gSize: BufferAttribute;
  private gMode: BufferAttribute;
  private cursor = 0;
  private count = 0;
  readonly mat: ShaderMaterial;

  constructor() {
    const g = new BufferGeometry();
    this.gPos = new BufferAttribute(new Float32Array(MAXP * 3), 3).setUsage(DynamicDrawUsage);
    this.gCol = new BufferAttribute(new Float32Array(MAXP * 4), 4).setUsage(DynamicDrawUsage);
    this.gSize = new BufferAttribute(new Float32Array(MAXP), 1).setUsage(DynamicDrawUsage);
    this.gMode = new BufferAttribute(new Float32Array(MAXP), 1).setUsage(DynamicDrawUsage);
    g.setAttribute('position', this.gPos);
    g.setAttribute('aColor', this.gCol);
    g.setAttribute('aSize', this.gSize);
    g.setAttribute('aMode', this.gMode);
    g.setDrawRange(0, 0);
    this.mat = additive(new ShaderMaterial({
      uniforms: { uScale: { value: 500 } },
      vertexShader: /* glsl */ `
        attribute vec4 aColor; attribute float aSize; attribute float aMode;
        uniform float uScale;
        varying vec4 vColor; varying float vMode;
        void main() {
          vec4 mv = modelViewMatrix * vec4(position, 1.0);
          gl_Position = projectionMatrix * mv;
          gl_PointSize = clamp(aSize * uScale / max(0.1, -mv.z), 1.0, 512.0);
          vColor = aColor; vMode = aMode;
        }`,
      fragmentShader: /* glsl */ `
        varying vec4 vColor; varying float vMode;
        void main() {
          float d = length(gl_PointCoord - 0.5) * 2.0;
          if (d > 1.0) discard;
          float a;
          if (vMode < 0.5) a = pow(1.0 - d, 2.2);
          else if (vMode < 1.5) a = smoothstep(0.62, 0.86, d) * (1.0 - smoothstep(0.88, 1.0, d));
          else a = 1.0 - smoothstep(0.55, 0.9, d);
          a *= vColor.a;
          gl_FragColor = vec4(vColor.rgb * a, a * 0.6);
        }`,
    }));
    this.points = new Points(g, this.mat);
    this.points.frustumCulled = false;
    this.points.renderOrder = 20;
  }

  spawn(o: ParticleOpts) {
    // find a free slot (ring buffer; overwrites the oldest when full)
    let i = this.cursor;
    for (let n = 0; n < MAXP; n++) {
      const k = (this.cursor + n) % MAXP;
      if (!this.alive[k]) { i = k; break; }
    }
    this.cursor = (i + 1) % MAXP;
    this.alive[i] = 1;
    this.pos.set([o.pos.x, o.pos.y, o.pos.z], i * 3);
    const v = o.vel ?? new Vector3();
    this.vel.set([v.x, v.y, v.z], i * 3);
    this.base.set([o.color.r, o.color.g, o.color.b, o.intensity ?? 1], i * 4);
    this.size[i * 2] = o.size;
    this.size[i * 2 + 1] = o.sizeEnd ?? o.size;
    this.life[i * 2] = 0;
    this.life[i * 2 + 1] = o.life;
    this.phys[i * 2] = o.drag ?? 0;
    this.phys[i * 2 + 1] = o.gravity ?? 0;
    this.mode[i] = o.mode ?? 0;
    this.streak[i] = o.streak ?? 0;
  }

  update(dt: number, beams: Beams) {
    let n = 0;
    const P = this.gPos.array as Float32Array, C = this.gCol.array as Float32Array, S = this.gSize.array as Float32Array, M = this.gMode.array as Float32Array;
    const a = new Vector3(), b = new Vector3(), c = new Color();
    for (let i = 0; i < MAXP; i++) {
      if (!this.alive[i]) continue;
      const age = (this.life[i * 2] += dt);
      const max = this.life[i * 2 + 1];
      if (age >= max) { this.alive[i] = 0; continue; }
      const t = age / max;
      const drag = Math.exp(-this.phys[i * 2] * dt);
      const vi = i * 3;
      this.vel[vi] *= drag; this.vel[vi + 1] = this.vel[vi + 1] * drag - this.phys[i * 2 + 1] * dt; this.vel[vi + 2] *= drag;
      this.pos[vi] += this.vel[vi] * dt; this.pos[vi + 1] += this.vel[vi + 1] * dt; this.pos[vi + 2] += this.vel[vi + 2] * dt;
      const fade = (1 - t) * (1 - t);
      const inten = this.base[i * 4 + 3] * fade;
      const sz = this.size[i * 2] + (this.size[i * 2 + 1] - this.size[i * 2]) * t;
      if (this.streak[i] > 0) {
        a.set(this.pos[vi], this.pos[vi + 1], this.pos[vi + 2]);
        b.set(this.vel[vi], this.vel[vi + 1], this.vel[vi + 2]).multiplyScalar(-this.streak[i]).add(a);
        c.setRGB(this.base[i * 4], this.base[i * 4 + 1], this.base[i * 4 + 2]);
        beams.push(a, b, sz, sz * 0.3, c, inten);
        continue;
      }
      P[n * 3] = this.pos[vi]; P[n * 3 + 1] = this.pos[vi + 1]; P[n * 3 + 2] = this.pos[vi + 2];
      C[n * 4] = this.base[i * 4] * inten; C[n * 4 + 1] = this.base[i * 4 + 1] * inten; C[n * 4 + 2] = this.base[i * 4 + 2] * inten; C[n * 4 + 3] = Math.min(1, fade * 1.2);
      S[n] = sz;
      M[n] = this.mode[i];
      n++;
    }
    this.count = n;
    this.points.geometry.setDrawRange(0, n);
    for (const attr of [this.gPos, this.gCol, this.gSize, this.gMode]) {
      attr.clearUpdateRanges();
      attr.addUpdateRange(0, n * attr.itemSize);
      attr.needsUpdate = true;
    }
  }

  get active() { return this.count; }
  clear() { this.alive.fill(0); }
}

// ------------------------------------------------------------------------------------------------
// Beams: camera-facing glowing ribbons between two points (tethers, forming beams, spark streaks,
// speed lines). Immediate mode: push every frame. One draw call.
// ------------------------------------------------------------------------------------------------

const MAXB = 2048;

export class Beams {
  readonly mesh: Mesh;
  private a: InstancedBufferAttribute;
  private b: InstancedBufferAttribute;
  private w: InstancedBufferAttribute;
  private c: InstancedBufferAttribute;
  private n = 0;
  private geo: InstancedBufferGeometry;

  constructor() {
    const g = new InstancedBufferGeometry();
    // x = side (-1..1), y = along (0..1)
    g.setAttribute('position', new BufferAttribute(new Float32Array([-1, 0, 0, 1, 0, 0, 1, 1, 0, -1, 1, 0]), 3));
    g.setIndex([0, 1, 2, 0, 2, 3]);
    this.a = new InstancedBufferAttribute(new Float32Array(MAXB * 3), 3).setUsage(DynamicDrawUsage);
    this.b = new InstancedBufferAttribute(new Float32Array(MAXB * 3), 3).setUsage(DynamicDrawUsage);
    this.w = new InstancedBufferAttribute(new Float32Array(MAXB * 2), 2).setUsage(DynamicDrawUsage);
    this.c = new InstancedBufferAttribute(new Float32Array(MAXB * 4), 4).setUsage(DynamicDrawUsage);
    g.setAttribute('iA', this.a);
    g.setAttribute('iB', this.b);
    g.setAttribute('iW', this.w);
    g.setAttribute('iC', this.c);
    g.instanceCount = 0;
    this.geo = g;
    const mat = additive(new ShaderMaterial({
      uniforms: { uTime: { value: 0 } },
      vertexShader: /* glsl */ `
        attribute vec3 iA; attribute vec3 iB; attribute vec2 iW; attribute vec4 iC;
        varying vec4 vC; varying vec2 vUv; varying float vLen;
        void main() {
          vec3 p = mix(iA, iB, position.y);
          vec3 dir = iB - iA;
          vec3 toCam = cameraPosition - p;
          vec3 side = cross(dir, toCam);
          float sl = length(side);
          side = sl > 1e-6 ? side / sl : vec3(0.0, 1.0, 0.0);
          float w = mix(iW.x, iW.y, position.y);
          // keep beams at least ~1.5px wide far away
          float minW = length(toCam) * 0.0016;
          p += side * max(w, minW) * position.x;
          vC = iC; vUv = position.xy; vLen = length(dir);
          gl_Position = projectionMatrix * viewMatrix * vec4(p, 1.0);
        }`,
      fragmentShader: /* glsl */ `
        uniform float uTime;
        varying vec4 vC; varying vec2 vUv; varying float vLen;
        void main() {
          float x = abs(vUv.x);
          float core = 1.0 - smoothstep(0.0, 0.35, x);
          float glow = pow(1.0 - x, 2.0);
          float a = (core * 0.9 + glow * 0.5) * vC.a;
          vec3 col = mix(vC.rgb, vec3(1.0), core * 0.6);
          gl_FragColor = vec4(col * a, a * 0.5);
        }`,
    }));
    this.mesh = new Mesh(g, mat);
    this.mesh.frustumCulled = false;
    this.mesh.renderOrder = 21;
  }

  begin() { this.n = 0; }

  push(a: Vector3, b: Vector3, wa: number, wb: number, color: Color, intensity: number) {
    if (this.n >= MAXB) return;
    const i = this.n++;
    (this.a.array as Float32Array).set([a.x, a.y, a.z], i * 3);
    (this.b.array as Float32Array).set([b.x, b.y, b.z], i * 3);
    (this.w.array as Float32Array).set([wa, wb], i * 2);
    (this.c.array as Float32Array).set([color.r * intensity, color.g * intensity, color.b * intensity, Math.min(1, intensity)], i * 4);
  }

  end(time: number) {
    this.geo.instanceCount = this.n;
    for (const attr of [this.a, this.b, this.w, this.c]) {
      attr.clearUpdateRanges();
      attr.addUpdateRange(0, this.n * attr.itemSize);
      attr.needsUpdate = true;
    }
    (this.mesh.material as ShaderMaterial).uniforms.uTime.value = time;
  }

  get count() { return this.n; }
}

// ------------------------------------------------------------------------------------------------
// Projectiles: instanced glowing spheres stretched along velocity. One draw call.
// ------------------------------------------------------------------------------------------------

const MAXPROJ = 1024;

export class ProjectileBatch {
  readonly mesh: Mesh;
  private p: InstancedBufferAttribute;
  private v: InstancedBufferAttribute;
  private c: InstancedBufferAttribute;
  private geo: InstancedBufferGeometry;
  private n = 0;

  constructor() {
    const s = new SphereGeometry(0.42, 10, 8);
    const g = new InstancedBufferGeometry();
    g.setAttribute('position', s.getAttribute('position'));
    g.setAttribute('normal', s.getAttribute('normal'));
    g.setIndex(s.index);
    this.p = new InstancedBufferAttribute(new Float32Array(MAXPROJ * 3), 3).setUsage(DynamicDrawUsage);
    this.v = new InstancedBufferAttribute(new Float32Array(MAXPROJ * 3), 3).setUsage(DynamicDrawUsage);
    this.c = new InstancedBufferAttribute(new Float32Array(MAXPROJ * 3), 3).setUsage(DynamicDrawUsage);
    g.setAttribute('iP', this.p);
    g.setAttribute('iV', this.v);
    g.setAttribute('iC', this.c);
    g.instanceCount = 0;
    this.geo = g;
    const mat = additive(new ShaderMaterial({
      vertexShader: /* glsl */ `
        attribute vec3 iP; attribute vec3 iV; attribute vec3 iC;
        varying vec3 vC; varying vec3 vN; varying vec3 vV;
        void main() {
          vec3 p = position;
          float sp = length(iV);
          vec3 d = sp > 0.1 ? iV / sp : vec3(0.0, 0.0, 1.0);
          float stretch = 1.0 + sp * 0.06;
          // stretch along velocity, trailing behind
          float along = dot(p, d);
          p += d * (along * (stretch - 1.0) - (stretch - 1.0) * 0.42);
          vec3 wp = iP + p;
          vC = iC; vN = normal; vV = cameraPosition - wp;
          gl_Position = projectionMatrix * viewMatrix * vec4(wp, 1.0);
        }`,
      fragmentShader: /* glsl */ `
        varying vec3 vC; varying vec3 vN; varying vec3 vV;
        void main() {
          float ndv = abs(dot(normalize(vN), normalize(vV)));
          vec3 col = mix(vC, vec3(1.0), pow(ndv, 3.0) * 0.8) * (1.2 + 2.2 * pow(ndv, 2.0));
          gl_FragColor = vec4(col, 0.8);
        }`,
    }));
    this.mesh = new Mesh(g, mat);
    this.mesh.frustumCulled = false;
    this.mesh.renderOrder = 19;
  }

  begin() { this.n = 0; }
  push(p: Vector3, v: Vector3, c: Color) {
    if (this.n >= MAXPROJ) return;
    const i = this.n++;
    (this.p.array as Float32Array).set([p.x, p.y, p.z], i * 3);
    (this.v.array as Float32Array).set([v.x, v.y, v.z], i * 3);
    (this.c.array as Float32Array).set([c.r, c.g, c.b], i * 3);
  }
  end() {
    this.geo.instanceCount = this.n;
    for (const attr of [this.p, this.v, this.c]) {
      attr.clearUpdateRanges();
      attr.addUpdateRange(0, this.n * attr.itemSize);
      attr.needsUpdate = true;
    }
  }
  get count() { return this.n; }
}

// ------------------------------------------------------------------------------------------------
// Effect presets
// ------------------------------------------------------------------------------------------------

const tmpV = new Vector3();
function rndDir(out = new Vector3()) {
  const u = Math.random() * 2 - 1, th = Math.random() * Math.PI * 2, s = Math.sqrt(1 - u * u);
  return out.set(s * Math.cos(th), u, s * Math.sin(th));
}

export class FX {
  readonly particles = new Particles();
  readonly beams = new Beams();
  readonly projectiles = new ProjectileBatch();
  private white = new Color(1, 1, 1);

  constructor(scene: Scene) {
    scene.add(this.particles.points, this.beams.mesh, this.projectiles.mesh);
  }

  setPointScale(viewportHeightPx: number, fovDeg: number) {
    this.particles.mat.uniforms.uScale.value = viewportHeightPx / (2 * Math.tan((fovDeg * Math.PI) / 360));
  }

  sparks(pos: Vector3, color: Color, n: number, speed: number, opts: { life?: number; up?: number; size?: number; normal?: Vector3 } = {}) {
    for (let i = 0; i < n; i++) {
      const d = rndDir();
      if (opts.normal && d.dot(opts.normal) < 0) d.reflect(opts.normal);
      if (opts.up) d.y = Math.abs(d.y) * opts.up + d.y * (1 - opts.up);
      this.particles.spawn({
        pos: pos.clone(), vel: d.multiplyScalar(speed * (0.35 + Math.random() * 0.8)), color, intensity: 2.5, size: opts.size ?? 0.06,
        life: (opts.life ?? 0.45) * (0.6 + Math.random() * 0.8), drag: 2.5, gravity: 14, streak: 0.045,
      });
    }
  }

  glow(pos: Vector3, color: Color, size: number, life: number, intensity = 2, sizeEnd?: number) {
    this.particles.spawn({ pos: pos.clone(), color, intensity, size, sizeEnd: sizeEnd ?? size * 1.6, life, mode: 0 });
  }

  ring(pos: Vector3, color: Color, size: number, sizeEnd: number, life: number, intensity = 2) {
    this.particles.spawn({ pos: pos.clone(), color, intensity, size, sizeEnd, life, mode: 1 });
  }

  impact(pos: Vector3, color: Color, strength = 1) {
    this.glow(pos, this.white, 1.4 * strength, 0.18, 3);
    this.glow(pos, color, 2.6 * strength, 0.35, 2);
    this.ring(pos, color, 0.8, 5 * strength, 0.35, 2.5);
    this.sparks(pos, color, Math.round(18 * strength), 16 * strength);
  }

  explosion(pos: Vector3, radius: number, color: Color) {
    this.glow(pos, this.white, radius * 0.6, 0.22, 4);
    this.glow(pos, color, radius * 1.1, 0.6, 2.5, radius * 1.8);
    this.ring(pos, color, radius * 0.3, radius * 2.4, 0.55, 3);
    this.ring(pos, this.white, radius * 0.2, radius * 1.4, 0.3, 2);
    this.sparks(pos, color, 60, radius * 3.5, { life: 0.8, size: 0.12 });
    const smoke = new Color(0.5, 0.45, 0.5);
    for (let i = 0; i < 14; i++) {
      this.particles.spawn({ pos: pos.clone().add(rndDir(tmpV).multiplyScalar(radius * 0.3)), vel: rndDir().multiplyScalar(4).add(new Vector3(0, 3, 0)), color: smoke, intensity: 0.25, size: radius * 0.5, sizeEnd: radius * 1.2, life: 1.4, drag: 1.2, mode: 0 });
    }
  }

  dust(pos: Vector3, amount: number, color = new Color(0.9, 0.86, 0.8)) {
    for (let i = 0; i < amount; i++) {
      const d = rndDir(); d.y = Math.abs(d.y) * 0.3;
      this.particles.spawn({ pos: pos.clone(), vel: d.multiplyScalar(3 + Math.random() * 4), color, intensity: 0.35, size: 0.5, sizeEnd: 1.6, life: 0.6 + Math.random() * 0.4, drag: 3, gravity: -0.5, mode: 0 });
    }
  }

  update(dt: number) { this.particles.update(dt, this.beams); }

  clear() { this.particles.clear(); }
}
