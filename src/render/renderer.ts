import {
  CircleGeometry, Color, Mesh, PCFShadowMap, PerspectiveCamera, Quaternion, Scene, ShaderMaterial, Vector3, WebGLRenderer,
  NoToneMapping, LinearSRGBColorSpace, BackSide, FrontSide, AddEquation, CustomBlending, OneFactor, OneMinusSrcAlphaFactor, ZeroFactor,
} from 'three';
import type { World, GameEvent } from '../sim/world.js';
import type { ConstructSpec } from '../sim/constructs/spec.js';
import { G, groups } from '../sim/physics.js';
import { QUALITY, HARDLIGHT_COLOR, ENTITY_COLORS, type RenderOptions } from './config.js';
import { Environment } from './environment.js';
import { buildStatics } from './statics.js';
import { PostPipeline } from './post.js';
import { FX } from './fx.js';
import { ConstructViews } from './constructs.js';
import { EntityViews } from './entities.js';
import { Markers } from './markers.js';
import { PlayerRig, type RigState } from './playerRig.js';
import { buildConstructGeometry, edgeMaterial, fillMaterial, ghostMaterial, hardlightUniforms } from './hardlight.js';
import { defaultSpec } from '../sim/constructs/spec.js';

export interface ViewInput { yaw: number; pitch: number; boosting?: boolean }

export interface RenderStats {
  sceneCalls: number;
  sceneTriangles: number;
  totalCalls: number;
  frameCpuMs: number; // JS time spent in render() (scene update + GL submission)
  updateMs: number; // scene graph update part
  particles: number;
  beams: number;
  projectiles: number;
  constructs: number;
  entities: number;
}

interface ProjSnap { prev: Vector3; curr: Vector3; vel: Vector3; reflected: boolean; seen: number }

const UP = new Vector3(0, 1, 0);

/** Owns the Three.js scene and draws a World. Purely a consumer of sim state. */
export class GameRenderer {
  readonly renderer: WebGLRenderer;
  readonly scene = new Scene();
  readonly camera: PerspectiveCamera;
  readonly env: Environment;
  readonly post: PostPipeline;
  readonly fx: FX;
  readonly constructs: ConstructViews;
  readonly entities: EntityViews;
  readonly markers: Markers;
  readonly rig: PlayerRig;
  world: World | null = null;
  private statics: Mesh | null = null;
  private opts: RenderOptions;
  private color: Color;
  private listener: ((e: GameEvent) => void) | null = null;
  // interpolation state
  private pPrev = new Vector3(); private pCurr = new Vector3();
  private vPrev = new Vector3(); private vCurr = new Vector3();
  private tPrev: Record<'L' | 'R', Vector3 | null> = { L: null, R: null };
  private tCurr: Record<'L' | 'R', Vector3 | null> = { L: null, R: null };
  private tState: Record<'L' | 'R', string | null> = { L: null, R: null };
  private projs = new Map<number, ProjSnap>();
  private blockedThisTick = 0;
  private gasPrev = 100;
  private boosting = 0;
  private hurt = 0;
  private fovKick = 0;
  private fovSmooth = 0;
  private castTarget: Vector3 | null = null;
  private castT = 0;
  private time = 0;
  private tickCount = 0;
  private dropMarker: Mesh;
  private blob: Mesh | null = null;
  private streakAcc = 0;
  private sceneReady = false;
  stats: RenderStats = { sceneCalls: 0, sceneTriangles: 0, totalCalls: 0, frameCpuMs: 0, updateMs: 0, particles: 0, beams: 0, projectiles: 0, constructs: 0, entities: 0 };
  /** Where the camera looked from last frame (for HUD projection). */
  readonly camPos = new Vector3();
  hideRig = false;

  constructor(readonly canvas: HTMLCanvasElement, opts: Partial<RenderOptions> = {}) {
    this.opts = { quality: 'high', fov: 75, constructColor: HARDLIGHT_COLOR, ...opts };
    const q = QUALITY[this.opts.quality];
    this.renderer = new WebGLRenderer({ canvas, antialias: false, alpha: false, powerPreference: 'high-performance', stencil: false, depth: true });
    this.renderer.setPixelRatio(Math.min(this.opts.pixelRatio ?? window.devicePixelRatio ?? 1, q.maxPixelRatio));
    this.renderer.toneMapping = NoToneMapping;
    this.renderer.outputColorSpace = LinearSRGBColorSpace; // composite pass encodes sRGB itself
    this.renderer.shadowMap.enabled = q.shadows;
    this.renderer.shadowMap.type = PCFShadowMap;
    this.renderer.info.autoReset = false;
    this.renderer.autoClear = false;
    this.camera = new PerspectiveCamera(this.opts.fov, 16 / 9, 0.1, 3000);
    this.color = new Color(this.opts.constructColor);
    this.env = new Environment(this.scene, this.opts.quality);
    this.post = new PostPipeline(this.renderer, q.bloomLevels, q.msaa, this.env.uniforms);
    this.fx = new FX(this.scene);
    this.constructs = new ConstructViews(this.opts.constructColor, this.fx);
    this.entities = new EntityViews(this.fx);
    this.markers = new Markers(this.fx);
    this.scene.add(this.constructs.root, this.entities.root, this.markers.root);
    this.rig = new PlayerRig(this.scene, this.color);
    this.dropMarker = this.makeDropMarker();
    this.scene.add(this.dropMarker);
    if (!q.shadows) {
      // no shadow map on low quality: a soft contact blob keeps the player grounded visually
      const bm = new ShaderMaterial({
        uniforms: { uA: { value: 0.5 } },
        vertexShader: `varying vec2 vUv; void main(){ vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position,1.0); }`,
        fragmentShader: `uniform float uA; varying vec2 vUv; void main(){ float d = length(vUv - 0.5) * 2.0; float a = (1.0 - smoothstep(0.2, 1.0, d)) * uA; gl_FragColor = vec4(0.05, 0.06, 0.12, a); }`,
        transparent: true, depthWrite: false, polygonOffset: true, polygonOffsetFactor: -1, polygonOffsetUnits: -1,
      });
      this.blob = new Mesh(new CircleGeometry(0.55, 24), bm);
      this.blob.renderOrder = 4;
      this.blob.frustumCulled = false;
      this.scene.add(this.blob);
    }
    this.resize();
    this.warmup();
  }

  /**
   * Compile the hard-light shader programs up front so the first construct doesn't hitch.
   * The warm-up meshes stay (invisible) so their programs are never released.
   */
  private warmup() {
    const geo = buildConstructGeometry(defaultSpec(), null, 1);
    const u = hardlightUniforms(this.color);
    const group = new Mesh();
    for (const [g, m] of [[geo.fill, fillMaterial(u, BackSide)], [geo.fill, fillMaterial(u, FrontSide)], [geo.edges, edgeMaterial(u)], [geo.fill, ghostMaterial(u)]] as const) {
      const mesh = new Mesh(g, m);
      mesh.frustumCulled = false;
      group.add(mesh);
    }
    this.scene.add(group);
    try { this.renderer.compile(this.scene, this.camera); } catch { /* compile is an optimisation only */ }
    group.visible = false;
  }

  private makeDropMarker(): Mesh {
    const m = new ShaderMaterial({
      uniforms: { uColor: { value: this.color.clone() }, uA: { value: 0 } },
      vertexShader: `varying vec2 vUv; void main(){ vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position,1.0); }`,
      fragmentShader: `uniform vec3 uColor; uniform float uA; varying vec2 vUv;
        void main(){ float d = length(vUv - 0.5) * 2.0; float ring = smoothstep(0.7, 0.82, d) * (1.0 - smoothstep(0.9, 1.0, d));
          float dot_ = 1.0 - smoothstep(0.12, 0.2, d); float a = (ring + dot_) * uA; gl_FragColor = vec4(uColor * 1.6 * a, a * 0.6); }`,
      transparent: true, depthWrite: false, polygonOffset: true, polygonOffsetFactor: -2, polygonOffsetUnits: -2,
    });
    m.blending = CustomBlending; m.blendEquation = AddEquation;
    m.blendSrc = OneFactor; m.blendDst = OneMinusSrcAlphaFactor; m.blendSrcAlpha = ZeroFactor; m.blendDstAlpha = OneMinusSrcAlphaFactor;
    const mesh = new Mesh(new CircleGeometry(0.6, 32), m);
    mesh.renderOrder = 5;
    mesh.frustumCulled = false;
    return mesh;
  }

  setConstructColor(hex: string) {
    this.color.set(hex);
    this.constructs.setColor(hex);
    this.rig.setRingColor(this.color);
    (this.dropMarker.material as ShaderMaterial).uniforms.uColor.value.copy(this.color);
  }

  setFov(f: number) { this.opts.fov = f; }

  resize() {
    const w = Math.max(1, this.canvas.clientWidth || window.innerWidth);
    const h = Math.max(1, this.canvas.clientHeight || window.innerHeight);
    this.renderer.setSize(w, h, false);
    const pr = this.renderer.getPixelRatio();
    this.post.setSize(Math.round(w * pr), Math.round(h * pr));
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.fx.setPointScale(h * pr, this.camera.fov);
  }

  /** Attach a (new) world: rebuild static geometry, markers, reset views. */
  setWorld(w: World) {
    if (this.world && this.listener) {
      const i = this.world.listeners.indexOf(this.listener);
      if (i >= 0) this.world.listeners.splice(i, 1);
    }
    this.world = w;
    if (this.statics) { this.scene.remove(this.statics); this.statics.geometry.dispose(); }
    this.statics = buildStatics(w.statics);
    this.scene.add(this.statics);
    this.constructs.clear();
    this.entities.clear();
    this.fx.clear();
    this.markers.build(w);
    this.projs.clear();
    this.listener = (e) => this.onEvent(e);
    w.listeners.push(this.listener);
    this.pPrev.copy(w.player.pos); this.pCurr.copy(w.player.pos);
    this.vPrev.set(0, 0, 0); this.vCurr.set(0, 0, 0);
    this.tPrev = { L: null, R: null }; this.tCurr = { L: null, R: null };
    this.gasPrev = w.player.gas;
    this.sceneReady = false;
    this.tick();
  }

  setPreview(specs: ConstructSpec[]) { this.constructs.setPreview(specs); }

  // ------------------------------------------------------------------------------------------ sim → views

  /** Call after every World.step(): snapshots poses for interpolation and consumes player events. */
  tick() {
    const w = this.world;
    if (!w) return;
    this.tickCount++;
    const P = w.player;
    this.pPrev.copy(this.pCurr); this.pCurr.copy(P.pos);
    this.vPrev.copy(this.vCurr); this.vCurr.copy(P.vel);
    for (const side of ['L', 'R'] as const) {
      const t = P.tethers[side];
      this.tPrev[side] = this.tCurr[side];
      this.tCurr[side] = t ? t.origin.clone() : null;
      if (!this.tPrev[side] && this.tCurr[side]) this.tPrev[side] = this.tCurr[side]!.clone();
      this.tState[side] = t ? t.state : null;
    }
    // boost detection: gas drained this tick
    this.boosting = P.gas < this.gasPrev - 0.01 ? 1 : Math.max(0, this.boosting - 0.2);
    this.gasPrev = P.gas;
    for (const ev of P.events) this.onPlayerEvent(ev);
    this.constructs.tick(w, this.rig.handR);
    this.entities.tick(w);
    this.markers.tick(w);
    // projectiles
    for (const pr of w.projectiles.values()) {
      const t = pr.body.translation();
      const v = pr.body.linvel();
      let s = this.projs.get(pr.id);
      if (!s) { s = { prev: new Vector3(t.x, t.y, t.z), curr: new Vector3(t.x, t.y, t.z), vel: new Vector3(), reflected: pr.reflected, seen: 0 }; this.projs.set(pr.id, s); }
      s.prev.copy(s.curr); s.curr.set(t.x, t.y, t.z); s.vel.set(v.x, v.y, v.z); s.reflected = pr.reflected; s.seen = this.tickCount;
    }
    for (const [id, s] of this.projs) {
      if (s.seen === this.tickCount) continue;
      // projectile vanished: small puff where it died (mint if a construct stopped it)
      const col = this.blockedThisTick > 0 || s.reflected ? this.color : new Color(ENTITY_COLORS.projectile);
      this.fx.glow(s.curr, col, 1.2, 0.2, 2.5);
      this.fx.sparks(s.curr, col, 8, 9, { size: 0.05, life: 0.3 });
      if (this.blockedThisTick > 0) this.fx.ring(s.curr, col, 0.3, 2.2, 0.25, 2);
      this.projs.delete(id);
    }
    this.blockedThisTick = 0;
  }

  private onPlayerEvent(ev: string) {
    const w = this.world!;
    const P = w.player;
    const feet = P.pos.clone().setY(P.pos.y - 0.9);
    if (ev === 'jump') this.fx.dust(feet, 4);
    else if (ev === 'land') this.fx.dust(feet, 6);
    else if (ev === 'land_hard') { this.fx.dust(feet, 16); this.fx.ring(feet.clone().add(new Vector3(0, 0.1, 0)), new Color(1, 1, 1), 0.5, 4, 0.35, 0.8); this.fovKick -= 3; }
    else if (ev.startsWith('tether_attach_')) {
      const side = ev.endsWith('L') ? 'L' : 'R';
      const t = P.tethers[side];
      if (t) { this.fx.glow(t.origin, this.color, 1.2, 0.25, 3); this.fx.sparks(t.origin, this.color, 10, 8, { size: 0.05 }); }
    } else if (ev.startsWith('tether_fire_')) {
      this.fx.glow(ev.endsWith('L') ? this.rig.handL : this.rig.handR, this.color, 0.6, 0.15, 3);
    } else if (ev === 'fly_on') { this.fx.ring(P.pos.clone(), this.color, 0.5, 5, 0.4, 2); this.fovKick += 4; }
  }

  private onEvent(e: GameEvent) {
    const w = this.world!;
    const pos = e.pos ? new Vector3(e.pos[0], e.pos[1], e.pos[2]) : null;
    switch (e.type) {
      case 'impact': if (pos) { this.fx.impact(pos, this.color, Math.min(1.8, 0.6 + (e.dv ?? 10) / 30)); this.fovKick += 1.5; } break;
      case 'smash': if (pos) { this.fx.impact(pos, this.color, 1.6); this.fx.dust(pos, 12); this.fovKick += 2; } break;
      case 'explosion': if (pos) { this.fx.explosion(pos, e.radius ?? 8, new Color('#ffb04d').lerp(this.color, 0.35)); this.fovKick += 5; } break;
      case 'launch': {
        const f = w.player.pos.clone().setY(w.player.pos.y - 0.9);
        this.fx.ring(f, this.color, 0.8, 7, 0.45, 3);
        this.fx.sparks(f, this.color, 24, 14, { up: 0.9, size: 0.06 });
        this.fovKick += 7;
        break;
      }
      case 'blocked': this.blockedThisTick++; break;
      case 'player_hit': this.hurt = 1; this.fx.sparks(w.player.pos, new Color('#ff5a4d'), 12, 8); break;
      case 'entity_damage': {
        this.entities.onDamage(e.id);
        const ent = w.entities.get(e.id);
        if (ent) this.fx.sparks(w.entityPos(ent), new Color('#ffe2a8'), 6, 8, { size: 0.05 });
        break;
      }
      case 'entity_disabled': {
        const ent = w.entities.get(e.id);
        if (ent) this.fx.explosion(w.entityPos(ent), 2.5, new Color('#ff8a3d'));
        break;
      }
      case 'fire': {
        this.entities.onFire(e.from);
        const m = this.entities.muzzle(e.from, new Vector3());
        if (m) this.fx.glow(m, new Color(ENTITY_COLORS.projectile), 1.4, 0.12, 3);
        break;
      }
      case 'construct_fire': {
        const c = w.constructs.get(e.construct);
        if (c?.insts[0]) this.fx.glow(c.insts[0].pos, this.color, 1.2, 0.15, 3);
        break;
      }
      case 'construct_formed': {
        const c = w.constructs.get(e.id);
        if (c?.insts[0]) { this.castTarget = c.insts[0].pos.clone(); this.castT = 0.55; }
        this.fx.glow(this.rig.handR, this.color, 1.5, 0.3, 4);
        this.fx.ring(this.rig.handR.clone(), this.color, 0.2, 2.2, 0.3, 3);
        break;
      }
      case 'construct_modified': {
        const c = w.constructs.get(e.id);
        if (c?.insts[0]) { this.castTarget = c.insts[0].pos.clone(); this.castT = 0.4; }
        break;
      }
      case 'construct_rejected':
        this.fx.sparks(this.rig.handR, new Color('#ff5a6a'), 14, 5, { size: 0.04, life: 0.35 });
        this.fx.glow(this.rig.handR, new Color('#ff5a6a'), 0.8, 0.25, 2);
        break;
      case 'construct_throw': this.fovKick += 1; break;
      case 'grabbed': if (e.entity) { const ent = w.entities.get(e.entity); if (ent) this.fx.ring(w.entityPos(ent), this.color, 0.5, 3, 0.3, 2.5); } break;
      case 'civilian_hurt': { const ent = w.entities.get(e.id); if (ent) this.fx.ring(w.entityPos(ent), new Color('#ff4d5e'), 0.5, 4, 0.6, 2.5); break; }
    }
  }

  // ------------------------------------------------------------------------------------------ frame

  /**
   * Draw one display frame. alpha = fraction of the way from the previous to the current sim tick.
   * view = the *live* mouse look (sampled at display rate, applied to the sim on the next tick).
   */
  render(alpha: number, frameDt: number, view: ViewInput) {
    const w = this.world;
    if (!w) return;
    const t0 = performance.now();
    this.time += frameDt;
    const dt = Math.min(0.1, frameDt);
    const P = w.player;
    const pos = this.pPrev.clone().lerp(this.pCurr, alpha);
    const vel = this.vPrev.clone().lerp(this.vCurr, alpha);
    const speed = vel.length();

    // ---- camera: the canonical sim camera evaluated at the interpolated pose + live look
    const save = { pos: P.pos.clone(), vel: P.vel.clone(), yaw: P.yaw, pitch: P.pitch };
    P.pos.copy(pos); P.vel.copy(vel); P.yaw = view.yaw; P.pitch = view.pitch;
    const cam = P.camera();
    const camPos = cam.pos.clone();
    const camDir = cam.dir.clone();
    P.pos.copy(save.pos); P.vel.copy(save.vel); P.yaw = save.yaw; P.pitch = save.pitch;
    this.camera.position.copy(camPos);
    this.camera.up.copy(UP);
    this.camera.lookAt(camPos.clone().add(camDir));
    this.fovKick *= Math.exp(-5 * dt);
    const wantFov = this.opts.fov + Math.min(20, Math.max(0, speed - 12) * 0.3) + this.fovKick;
    this.fovSmooth = this.fovSmooth ? this.fovSmooth + (wantFov - this.fovSmooth) * (1 - Math.exp(-6 * dt)) : wantFov;
    if (Math.abs(this.camera.fov - this.fovSmooth) > 0.01) {
      this.camera.fov = this.fovSmooth;
      this.camera.updateProjectionMatrix();
      this.fx.setPointScale(this.post.rt.height, this.camera.fov);
    }
    this.camera.updateMatrixWorld();
    this.camPos.copy(camPos);

    // ---- player rig
    this.castT = Math.max(0, this.castT - dt);
    const tL = this.tCurr.L && this.tPrev.L ? this.tPrev.L.clone().lerp(this.tCurr.L, alpha) : null;
    const tR = this.tCurr.R && this.tPrev.R ? this.tPrev.R.clone().lerp(this.tCurr.R, alpha) : null;
    const rs: RigState = {
      pos, vel, lookYaw: view.yaw, lookPitch: view.pitch, onGround: P.onGround, sliding: P.sliding, flying: P.flying,
      boosting: this.boosting > 0.5, tetherL: tL, tetherR: tR, castTarget: this.castT > 0 ? this.castTarget : null,
    };
    this.rig.update(rs, dt, this.time);
    // hide the rig when the camera is pulled inside it
    const tooClose = camPos.distanceTo(pos) < 0.9;
    this.rig.setVisible(!tooClose && !this.hideRig);

    // ---- views
    this.fx.beams.begin();
    this.fx.projectiles.begin();
    this.constructs.frame(alpha, dt, this.time, this.rig.handR);
    this.constructs.frameGhosts(w, this.time);
    this.entities.frame(alpha, dt, this.time, pos);
    this.markers.frame(dt, this.time);
    for (const s of this.projs.values()) {
      const p = s.prev.clone().lerp(s.curr, alpha);
      this.fx.projectiles.push(p, s.vel, s.reflected ? this.color : new Color(ENTITY_COLORS.projectile));
    }
    // tethers
    for (const side of ['L', 'R'] as const) {
      const tp = side === 'L' ? tL : tR;
      if (!tp) continue;
      const hand = side === 'L' ? this.rig.handL : this.rig.handR;
      const attached = this.tState[side] === 'attached';
      const pulse = 0.85 + 0.15 * Math.sin(this.time * 30);
      this.fx.beams.push(hand, tp, 0.06, 0.08, this.color, (attached ? 2.6 : 1.8) * pulse);
      if (attached && Math.random() < dt * 20) this.fx.glow(tp, this.color, 0.7, 0.15, 2);
      if (!attached) this.fx.glow(tp, this.color, 0.35, 0.05, 3);
    }
    // boost thruster
    if (this.boosting > 0.5) {
      const dir = vel.lengthSq() > 1 ? vel.clone().normalize().negate() : new Vector3(0, -1, 0);
      for (let i = 0; i < 2; i++) {
        this.fx.particles.spawn({ pos: this.rig.back.clone().add(new Vector3((Math.random() - 0.5) * 0.2, (Math.random() - 0.5) * 0.2, (Math.random() - 0.5) * 0.2)), vel: dir.clone().multiplyScalar(8 + Math.random() * 6).add(vel.clone().multiplyScalar(0.6)), color: this.color, intensity: 2.2, size: 0.18, sizeEnd: 0.02, life: 0.25, drag: 3, mode: 0 });
      }
      this.fx.beams.push(this.rig.back, this.rig.back.clone().addScaledVector(dir, 0.9 + Math.random() * 0.3), 0.14, 0.01, this.color, 2.4);
    }
    // wind streaks at speed
    if (speed > 20) {
      this.streakAcc += dt * Math.min(90, (speed - 20) * 2.2);
      const vdir = vel.clone().normalize();
      while (this.streakAcc > 1) {
        this.streakAcc -= 1;
        const off = new Vector3(Math.random() - 0.5, Math.random() - 0.5, Math.random() - 0.5).multiplyScalar(14);
        const p = pos.clone().addScaledVector(vdir, 8 + Math.random() * 14).add(off);
        this.fx.particles.spawn({ pos: p, vel: vdir.clone().multiplyScalar(-speed * 0.05), color: new Color(1, 1, 1), intensity: 0.35, size: 0.02, life: 0.35, streak: 2.2 + speed * 0.012 });
      }
    }
    this.fx.update(dt);
    this.fx.beams.end(this.time);
    this.fx.projectiles.end();

    // drop marker: shows where you'll land when high up
    this.updateDropMarker(w, pos, dt);

    this.env.update(this.camera, pos, this.time);
    const tUpd = performance.now();

    // ---- draw
    const info = this.renderer.info;
    info.reset();
    this.post.renderScene(this.scene, this.camera);
    const sceneCalls = info.render.calls;
    const sceneTris = info.render.triangles;
    this.hurt = Math.max(0, this.hurt - dt * 2.5);
    const speedFx = Math.min(1, Math.max(0, (speed - 24) / 36));
    this.post.finish(this.camera, { speed: speedFx, hurt: this.hurt, boost: this.boosting > 0.5 ? 1 : 0, time: this.time });
    const t1 = performance.now();
    this.stats = {
      sceneCalls, sceneTriangles: sceneTris, totalCalls: info.render.calls, frameCpuMs: t1 - t0, updateMs: tUpd - t0,
      particles: this.fx.particles.active, beams: this.fx.beams.count, projectiles: this.fx.projectiles.count,
      constructs: this.constructs.drawCount, entities: this.entities.count,
    };
    this.sceneReady = true;
  }

  private updateDropMarker(w: World, pos: Vector3, dt: number) {
    const R = w.R;
    const m = this.dropMarker.material as ShaderMaterial;
    const hit = w.pw.castRayAndGetNormal(new R.Ray(pos, { x: 0, y: -1, z: 0 }), 300, true, undefined, groups(G.PLAYER, G.WORLD | G.CONSTRUCT), w.player.collider);
    let want = 0;
    if (hit) {
      const h = hit.timeOfImpact - 0.9;
      const n = new Vector3(hit.normal.x, hit.normal.y, hit.normal.z);
      this.dropMarker.position.copy(pos).setY(pos.y - hit.timeOfImpact + 0.03);
      this.dropMarker.quaternion.copy(new Quaternion().setFromUnitVectors(new Vector3(0, 0, 1), n));
      want = h > 2 ? Math.min(1, (h - 2) / 4) : 0;
      this.dropMarker.scale.setScalar(1 + Math.min(2, h * 0.03));
      if (this.blob) {
        this.blob.position.copy(this.dropMarker.position).addScaledVector(n, 0.01);
        this.blob.quaternion.copy(this.dropMarker.quaternion);
        this.blob.scale.setScalar(1 + Math.min(1.5, h * 0.08));
        (this.blob.material as ShaderMaterial).uniforms.uA.value = 0.55 * Math.max(0, 1 - h / 12);
      }
    }
    if (this.blob) this.blob.visible = !!hit && !this.hideRig;
    m.uniforms.uA.value += (want - m.uniforms.uA.value) * Math.min(1, dt * 10);
    this.dropMarker.visible = m.uniforms.uA.value > 0.01;
  }

  get ready() { return this.sceneReady; }

  /** Project a world point to CSS pixel coordinates; z > 1 means behind the camera. */
  project(p: Vector3): { x: number; y: number; behind: boolean } {
    const v = p.clone().project(this.camera);
    const w = this.canvas.clientWidth, h = this.canvas.clientHeight;
    const behind = v.z > 1 || new Vector3().subVectors(p, this.camera.position).dot(this.camera.getWorldDirection(new Vector3())) < 0;
    return { x: (v.x * 0.5 + 0.5) * w, y: (-v.y * 0.5 + 0.5) * h, behind };
  }

  toCameraSpace(p: Vector3): Vector3 { return p.clone().applyMatrix4(this.camera.matrixWorldInverse); }

  dispose() {
    this.post.dispose();
    this.renderer.dispose();
  }
}
