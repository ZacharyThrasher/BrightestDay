import {
  AddEquation, Color, CustomBlending, CylinderGeometry, DoubleSide, Group, Mesh, OneFactor, OneMinusSrcAlphaFactor, Quaternion,
  ShaderMaterial, TorusGeometry, Vector3, ZeroFactor,
} from 'three';
import type { World } from '../sim/world.js';
import { GOAL_COLOR, RING_COLOR, RING_NEXT_COLOR } from './config.js';
import type { FX } from './fx.js';

function markerMat(color: string, kind: 'ring' | 'beacon') {
  const m = new ShaderMaterial({
    uniforms: { uColor: { value: new Color(color) }, uTime: { value: 0 }, uAlpha: { value: 1 }, uBoost: { value: 1 } },
    vertexShader: /* glsl */ `varying vec3 vN; varying vec3 vV; varying vec3 vL;
      void main(){ vec4 wp = modelMatrix * vec4(position,1.0); vL = position; vN = normalize(mat3(modelMatrix)*normal); vV = cameraPosition - wp.xyz; gl_Position = projectionMatrix * viewMatrix * wp; }`,
    fragmentShader: kind === 'ring' ? /* glsl */ `
      uniform vec3 uColor; uniform float uTime; uniform float uAlpha; uniform float uBoost; varying vec3 vN; varying vec3 vV; varying vec3 vL;
      void main(){
        float f = abs(dot(normalize(vN), normalize(vV)));
        float ang = atan(vL.z, vL.x);
        float chase = 0.5 + 0.5 * sin(ang * 6.0 - uTime * 6.0);
        vec3 c = uColor * (1.1 + 0.9 * pow(f, 2.0) + chase * 0.6) * uBoost;
        float a = uAlpha * (0.75 + 0.25 * f);
        gl_FragColor = vec4(c * a, a);
      }` : /* glsl */ `
      uniform vec3 uColor; uniform float uTime; uniform float uAlpha; uniform float uBoost; varying vec3 vN; varying vec3 vV; varying vec3 vL;
      void main(){
        float f = 1.0 - abs(dot(normalize(vN), normalize(vV)));
        float h = vL.y + 0.5; // 0 bottom .. 1 top
        float bands = 0.5 + 0.5 * sin(h * 40.0 - uTime * 4.0);
        float a = (0.1 + 0.35 * pow(f, 1.5)) * (1.0 - h) * (0.7 + 0.3 * bands) * uAlpha;
        vec3 c = uColor * (1.2 + bands * 0.5) * uBoost;
        gl_FragColor = vec4(c * a, a);
      }`,
    transparent: true,
    depthWrite: false,
    side: DoubleSide,
  });
  m.blending = CustomBlending;
  m.blendEquation = AddEquation;
  m.blendSrc = OneFactor; m.blendDst = OneMinusSrcAlphaFactor; m.blendSrcAlpha = ZeroFactor; m.blendDstAlpha = OneMinusSrcAlphaFactor;
  return m;
}

interface RingView { id: string; mesh: Mesh; mat: ShaderMaterial; passed: boolean; flash: number }
interface GoalView { id: string; group: Group; mat: ShaderMaterial; base: Mesh; reached: boolean }

/** Race rings and goal beacons. */
export class Markers {
  readonly root = new Group();
  private rings: RingView[] = [];
  private goals: GoalView[] = [];
  nextRingId: string | null = null;

  constructor(private fx: FX) { this.root.name = 'markers'; }

  build(w: World) {
    this.clear();
    for (const r of w.rings) {
      const mat = markerMat(RING_COLOR, 'ring');
      const mesh = new Mesh(new TorusGeometry(r.radius, 0.35, 10, 48), mat);
      mesh.position.copy(r.pos);
      // torus lies in XY (normal +Z) → rotate +Z onto ring normal
      mesh.quaternion.copy(new Quaternion().setFromUnitVectors(new Vector3(0, 0, 1), r.normal));
      mesh.renderOrder = 15;
      this.root.add(mesh);
      this.rings.push({ id: r.id, mesh, mat, passed: r.passed, flash: 0 });
    }
    for (const g of w.goals) {
      const size = g.max.clone().sub(g.min);
      const c = g.min.clone().add(g.max).multiplyScalar(0.5);
      const group = new Group();
      const mat = markerMat(GOAL_COLOR, 'beacon');
      const r = Math.max(2, Math.min(size.x, size.z) / 2);
      const beacon = new Mesh(new CylinderGeometry(r, r, 1, 40, 1, true), mat);
      const h = Math.max(30, size.y * 4);
      beacon.scale.set(1, h, 1);
      beacon.position.set(0, h / 2 - size.y / 2, 0);
      beacon.renderOrder = 16;
      group.add(beacon);
      const ringMat = markerMat(GOAL_COLOR, 'ring');
      const base = new Mesh(new TorusGeometry(r, 0.18, 6, 64), ringMat);
      base.rotation.x = Math.PI / 2;
      base.position.y = -size.y / 2 + 0.15;
      base.renderOrder = 16;
      group.add(base);
      group.position.copy(c);
      this.root.add(group);
      this.goals.push({ id: g.id, group, mat, base, reached: g.reached });
    }
  }

  tick(w: World) {
    const next = w.rings.find((r) => !r.passed);
    this.nextRingId = next?.id ?? null;
    for (const rv of this.rings) {
      const r = w.rings.find((x) => x.id === rv.id);
      if (r && r.passed && !rv.passed) {
        rv.passed = true;
        rv.flash = 1;
        this.fx.ring(r.pos.clone(), new Color(RING_COLOR), r.radius * 1.5, r.radius * 4, 0.5, 2.5);
        this.fx.sparks(r.pos.clone(), new Color(RING_COLOR), 40, 22, { life: 0.6, size: 0.08 });
      }
    }
    for (const gv of this.goals) {
      const g = w.goals.find((x) => x.id === gv.id);
      if (g && g.reached && !gv.reached) {
        gv.reached = true;
        const p = gv.group.position.clone();
        this.fx.ring(p, new Color(GOAL_COLOR), 2, 18, 0.9, 3);
        for (let i = 0; i < 3; i++) this.fx.sparks(p.clone().add(new Vector3(0, i * 2, 0)), new Color(i % 2 ? '#ffd35c' : '#ff7ad9'), 30, 18, { life: 1.2, up: 0.8, size: 0.1 });
      }
    }
  }

  frame(dt: number, time: number) {
    for (const rv of this.rings) {
      const isNext = rv.id === this.nextRingId;
      rv.flash = Math.max(0, rv.flash - dt * 2);
      rv.mat.uniforms.uTime.value = time;
      rv.mat.uniforms.uColor.value.set(isNext ? RING_NEXT_COLOR : RING_COLOR);
      rv.mat.uniforms.uAlpha.value = rv.passed ? 0.25 + rv.flash * 0.75 : 1;
      rv.mat.uniforms.uBoost.value = isNext ? 1.4 + 0.4 * Math.sin(time * 6) : rv.passed ? 0.6 + rv.flash * 2 : 1;
      rv.mesh.scale.setScalar(1 + rv.flash * 0.3);
    }
    for (const gv of this.goals) {
      gv.mat.uniforms.uTime.value = time;
      gv.mat.uniforms.uColor.value.set(gv.reached ? '#9dffc9' : GOAL_COLOR);
      gv.mat.uniforms.uAlpha.value = gv.reached ? 0.5 : 1;
      (gv.base.material as ShaderMaterial).uniforms.uTime.value = time;
    }
  }

  clear() {
    for (const r of this.rings) { this.root.remove(r.mesh); r.mesh.geometry.dispose(); r.mat.dispose(); }
    for (const g of this.goals) { this.root.remove(g.group); g.mat.dispose(); }
    this.rings = [];
    this.goals = [];
  }
}
