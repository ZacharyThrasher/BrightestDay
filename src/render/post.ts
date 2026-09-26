import {
  Color, DepthTexture, FloatType, HalfFloatType, LinearFilter, Matrix4, NearestFilter, NoBlending, RGBAFormat, ShaderMaterial,
  Vector2, Vector3, WebGLRenderTarget, type Camera, type PerspectiveCamera, type Scene, type WebGLRenderer,
} from 'three';
import { FullScreenQuad } from 'three/addons/postprocessing/Pass.js';
import { SKY_GLSL, NOISE_GLSL } from './glsl.js';
import { SKY, INK } from './config.js';
import type { skyUniforms } from './environment.js';

const FS_VERT = /* glsl */ `varying vec2 vUv; void main() { vUv = uv; gl_Position = vec4(position.xy, 0.0, 1.0); }`;

function pass(frag: string, uniforms: Record<string, { value: any }>) {
  return new ShaderMaterial({ uniforms, vertexShader: FS_VERT, fragmentShader: frag, depthTest: false, depthWrite: false, blending: NoBlending });
}

export interface PostParams {
  speed: number; // 0..1 speed-line intensity
  hurt: number; // 0..1 red edge flash
  boost: number; // 0..1 mint edge tint while boosting
  time: number;
}

/**
 * Custom post chain (cheaper + more controllable than EffectComposer):
 *   scene → HDR MSAA target (+ depth texture)
 *   dual-filter bloom (threshold > 1 so only energy/sun blooms)
 *   composite: depth-based ink silhouettes, sky-matched height fog, bloom, speed lines, vignette,
 *              soft highlight shoulder, sRGB + dither.
 * Alpha channel of the scene target is a mask: 1 = world (fog + ink), 0 = energy (constructs/FX).
 */
export class PostPipeline {
  readonly rt: WebGLRenderTarget;
  private down: WebGLRenderTarget[] = [];
  private up: WebGLRenderTarget[] = [];
  private quad = new FullScreenQuad();
  private prefilter: ShaderMaterial;
  private downMat: ShaderMaterial;
  private upMat: ShaderMaterial;
  private composite: ShaderMaterial;
  private size = new Vector2(1, 1);
  bloomStrength = 0.9;

  constructor(private renderer: WebGLRenderer, private levels: number, msaa: number, sky: ReturnType<typeof skyUniforms>) {
    const depth = new DepthTexture(1, 1, FloatType);
    depth.minFilter = NearestFilter;
    depth.magFilter = NearestFilter;
    this.rt = new WebGLRenderTarget(1, 1, { type: HalfFloatType, format: RGBAFormat, samples: msaa, depthTexture: depth, depthBuffer: true });
    this.rt.texture.minFilter = LinearFilter;
    this.rt.texture.generateMipmaps = false;
    for (let i = 0; i < levels; i++) {
      this.down.push(new WebGLRenderTarget(1, 1, { type: HalfFloatType, depthBuffer: false }));
      this.up.push(new WebGLRenderTarget(1, 1, { type: HalfFloatType, depthBuffer: false }));
    }
    this.prefilter = pass(/* glsl */ `
      uniform sampler2D tSrc; uniform vec2 uTexel; uniform float uThreshold; varying vec2 vUv;
      vec3 pre(vec3 c) { float b = max(c.r, max(c.g, c.b)); float s = max(0.0, b - uThreshold); s = s * s / (b * 0.5 + s + 1e-4); return c * (s / max(b, 1e-4)); }
      void main() {
        vec3 a = pre(texture2D(tSrc, vUv + uTexel * vec2(-1.0, -1.0)).rgb);
        vec3 b = pre(texture2D(tSrc, vUv + uTexel * vec2(1.0, -1.0)).rgb);
        vec3 c = pre(texture2D(tSrc, vUv + uTexel * vec2(-1.0, 1.0)).rgb);
        vec3 d = pre(texture2D(tSrc, vUv + uTexel * vec2(1.0, 1.0)).rgb);
        // Karis-style weighting tames fireflies
        float wa = 1.0 / (1.0 + max(a.r, max(a.g, a.b))), wb = 1.0 / (1.0 + max(b.r, max(b.g, b.b)));
        float wc = 1.0 / (1.0 + max(c.r, max(c.g, c.b))), wd = 1.0 / (1.0 + max(d.r, max(d.g, d.b)));
        gl_FragColor = vec4((a * wa + b * wb + c * wc + d * wd) / (wa + wb + wc + wd), 1.0);
      }`, { tSrc: { value: null }, uTexel: { value: new Vector2() }, uThreshold: { value: 1.0 } });
    this.downMat = pass(/* glsl */ `
      uniform sampler2D tSrc; uniform vec2 uTexel; varying vec2 vUv;
      void main() {
        vec3 s = texture2D(tSrc, vUv).rgb * 4.0;
        s += texture2D(tSrc, vUv + uTexel * vec2(-1.0, -1.0)).rgb;
        s += texture2D(tSrc, vUv + uTexel * vec2(1.0, -1.0)).rgb;
        s += texture2D(tSrc, vUv + uTexel * vec2(-1.0, 1.0)).rgb;
        s += texture2D(tSrc, vUv + uTexel * vec2(1.0, 1.0)).rgb;
        gl_FragColor = vec4(s / 8.0, 1.0);
      }`, { tSrc: { value: null }, uTexel: { value: new Vector2() } });
    this.upMat = pass(/* glsl */ `
      uniform sampler2D tSrc; uniform sampler2D tAdd; uniform vec2 uTexel; uniform float uMix; varying vec2 vUv;
      void main() {
        vec3 s = vec3(0.0);
        s += texture2D(tSrc, vUv + uTexel * vec2(-2.0, 0.0)).rgb;
        s += texture2D(tSrc, vUv + uTexel * vec2(2.0, 0.0)).rgb;
        s += texture2D(tSrc, vUv + uTexel * vec2(0.0, -2.0)).rgb;
        s += texture2D(tSrc, vUv + uTexel * vec2(0.0, 2.0)).rgb;
        s += texture2D(tSrc, vUv + uTexel * vec2(-1.0, -1.0)).rgb * 2.0;
        s += texture2D(tSrc, vUv + uTexel * vec2(1.0, -1.0)).rgb * 2.0;
        s += texture2D(tSrc, vUv + uTexel * vec2(-1.0, 1.0)).rgb * 2.0;
        s += texture2D(tSrc, vUv + uTexel * vec2(1.0, 1.0)).rgb * 2.0;
        gl_FragColor = vec4(s / 12.0 + texture2D(tAdd, vUv).rgb * uMix, 1.0);
      }`, { tSrc: { value: null }, tAdd: { value: null }, uTexel: { value: new Vector2() }, uMix: { value: 1 } });
    this.composite = pass(/* glsl */ `
      uniform sampler2D tScene; uniform sampler2D tDepth; uniform sampler2D tBloom;
      uniform vec2 uRes; uniform float uNear; uniform float uFar; uniform mat4 uInvProj; uniform mat4 uCamWorld; uniform vec3 uCamPos;
      uniform float uFogStart; uniform float uFogDensity; uniform float uFogHeight; uniform float uBloom; uniform float uTime;
      uniform float uSpeed; uniform float uHurt; uniform float uBoost; uniform float uOutlinePx; uniform vec3 uInk;
      ${SKY_GLSL}
      varying vec2 vUv;
      float viewZ(float d) { return (uNear * uFar) / ((uFar - uNear) * d - uFar); } // negative
      float invZ(vec2 uv) { return -1.0 / viewZ(texture2D(tDepth, uv).x); }
      vec3 shoulder(vec3 c) {
        vec3 k = 0.78 + 0.22 * (1.0 - exp(-(c - 0.78) / 0.22));
        return mix(c, k, step(0.78, c));
      }
      float lin2srgb(float c) { return c <= 0.0031308 ? 12.92 * c : 1.055 * pow(c, 1.0 / 2.4) - 0.055; }
      void main() {
        vec4 sc = texture2D(tScene, vUv);
        float d = texture2D(tDepth, vUv).x;
        vec3 col = sc.rgb;
        float mask = clamp(sc.a, 0.0, 1.0);
        vec4 vp4 = uInvProj * vec4(vUv * 2.0 - 1.0, d * 2.0 - 1.0, 1.0);
        vec3 vp = vp4.xyz / vp4.w;
        vec3 wdir = normalize((uCamWorld * vec4(vp, 0.0)).xyz);
        if (d < 1.0) {
          // ink silhouettes: Laplacian of 1/z is zero on planes at any angle, large at depth jumps
          vec2 px = uOutlinePx / uRes;
          float c0 = -1.0 / viewZ(d);
          float l = invZ(vUv - vec2(px.x, 0.0)), r = invZ(vUv + vec2(px.x, 0.0));
          float u = invZ(vUv + vec2(0.0, px.y)), dn = invZ(vUv - vec2(0.0, px.y));
          float lap = (l + r + u + dn - 4.0 * c0) / c0;
          float edge = smoothstep(0.05, 0.22, -lap) * mask;
          float dist = length(vp);
          edge *= 1.0 - smoothstep(250.0, 700.0, dist);
          col = mix(col, uInk * 0.9 + col * 0.18, edge * 0.9);
          // sky-matched height fog
          float wy = uCamPos.y + wdir.y * dist;
          float fog = 1.0 - exp(-max(0.0, dist - uFogStart) * uFogDensity);
          fog *= exp(-max(0.0, wy) * uFogHeight);
          fog *= mix(0.3, 1.0, mask);
          col = mix(col, skyGradient(wdir), clamp(fog, 0.0, 1.0));
        }
        col += texture2D(tBloom, vUv).rgb * uBloom;
        // speed lines: radial dashes that stream outward, stronger toward the edges
        vec2 p = (vUv - 0.5) * vec2(uRes.x / uRes.y, 1.0);
        float rad = length(p);
        if (uSpeed > 0.001) {
          float ang = atan(p.y, p.x);
          float k = floor(ang * 70.0 / 3.14159);
          float h = hash12(vec2(k, 3.0));
          float cw = abs(fract(ang * 70.0 / 3.14159) - 0.5);
          float dash = fract(rad * 1.8 - uTime * (2.5 + h * 3.0) + h * 7.0);
          float on = step(0.72, h) * smoothstep(0.35, 0.0, dash) * (1.0 - smoothstep(0.08, 0.25, cw));
          float radial = smoothstep(0.28, 0.75, rad);
          col = mix(col, vec3(1.0), on * radial * uSpeed * 0.35);
        }
        // vignette + damage / boost edge tint
        float vig = smoothstep(0.95, 0.35, rad);
        col *= mix(0.78, 1.0, vig);
        col = mix(col, vec3(1.0, 0.15, 0.2), (1.0 - vig) * uHurt * 0.55);
        col = mix(col, vec3(0.3, 1.0, 0.72), (1.0 - vig) * uBoost * 0.18);
        col = shoulder(max(col, 0.0));
        col = vec3(lin2srgb(col.r), lin2srgb(col.g), lin2srgb(col.b));
        col += (hash12(gl_FragCoord.xy + fract(uTime) * 91.0) - 0.5) / 255.0;
        gl_FragColor = vec4(col, 1.0);
      }`, {
      ...sky,
      tScene: { value: null }, tDepth: { value: null }, tBloom: { value: null },
      uRes: { value: new Vector2() }, uNear: { value: 0.1 }, uFar: { value: 3000 }, uInvProj: { value: new Matrix4() }, uCamWorld: { value: new Matrix4() },
      uCamPos: { value: new Vector3() }, uFogStart: { value: SKY.fogStart }, uFogDensity: { value: SKY.fogDensity }, uFogHeight: { value: SKY.fogHeightFalloff },
      uBloom: { value: this.bloomStrength }, uSpeed: { value: 0 }, uHurt: { value: 0 }, uBoost: { value: 0 },
      uOutlinePx: { value: 1 }, uInk: { value: new Color(INK) },
    });
    void NOISE_GLSL;
  }

  setSize(w: number, h: number) {
    this.size.set(w, h);
    this.rt.setSize(w, h);
    for (let i = 0; i < this.levels; i++) {
      const lw = Math.max(1, w >> (i + 1)), lh = Math.max(1, h >> (i + 1));
      this.down[i].setSize(lw, lh);
      this.up[i].setSize(lw, lh);
    }
    this.composite.uniforms.uRes.value.set(w, h);
    this.composite.uniforms.uOutlinePx.value = Math.max(1, h / 1000);
  }

  /** Render the scene into the HDR target. Returns nothing; stats are read by the caller. */
  renderScene(scene: Scene, camera: Camera) {
    const r = this.renderer;
    r.setRenderTarget(this.rt);
    r.clear(true, true, false);
    r.render(scene, camera);
  }

  finish(camera: PerspectiveCamera, p: PostParams) {
    const r = this.renderer;
    // bloom
    const q = this.quad;
    q.material = this.prefilter;
    this.prefilter.uniforms.tSrc.value = this.rt.texture;
    this.prefilter.uniforms.uTexel.value.set(0.5 / this.size.x, 0.5 / this.size.y);
    r.setRenderTarget(this.down[0]);
    q.render(r);
    q.material = this.downMat;
    for (let i = 1; i < this.levels; i++) {
      this.downMat.uniforms.tSrc.value = this.down[i - 1].texture;
      this.downMat.uniforms.uTexel.value.set(0.5 / this.down[i - 1].width, 0.5 / this.down[i - 1].height);
      r.setRenderTarget(this.down[i]);
      q.render(r);
    }
    q.material = this.upMat;
    for (let i = this.levels - 2; i >= 0; i--) {
      const src = i === this.levels - 2 ? this.down[i + 1] : this.up[i + 1];
      this.upMat.uniforms.tSrc.value = src.texture;
      this.upMat.uniforms.tAdd.value = this.down[i].texture;
      this.upMat.uniforms.uTexel.value.set(0.5 / src.width, 0.5 / src.height);
      r.setRenderTarget(this.up[i]);
      q.render(r);
    }
    // composite to screen
    const c = this.composite.uniforms;
    c.tScene.value = this.rt.texture;
    c.tDepth.value = this.rt.depthTexture;
    c.tBloom.value = this.levels > 1 ? this.up[0].texture : this.down[0].texture;
    c.uNear.value = camera.near;
    c.uFar.value = camera.far;
    c.uInvProj.value.copy(camera.projectionMatrixInverse);
    c.uCamWorld.value.copy(camera.matrixWorld);
    c.uCamPos.value.setFromMatrixPosition(camera.matrixWorld);
    c.uBloom.value = this.bloomStrength;
    c.uSpeed.value = p.speed;
    c.uHurt.value = p.hurt;
    c.uBoost.value = p.boost;
    c.uTime.value = p.time;
    q.material = this.composite;
    r.setRenderTarget(null);
    q.render(r);
  }

  dispose() {
    this.rt.dispose();
    this.down.forEach((t) => t.dispose());
    this.up.forEach((t) => t.dispose());
    this.quad.dispose();
  }
}
