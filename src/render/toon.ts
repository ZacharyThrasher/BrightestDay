import { Color, DataTexture, MeshToonMaterial, NearestFilter, RedFormat, type Material } from 'three';
import { INK } from './config.js';

let gradient: DataTexture | null = null;

/** 3-band toon ramp: shadow / mid / lit. Kept bright so the pastel city never goes muddy. */
export function toonGradient(): DataTexture {
  if (gradient) return gradient;
  const data = new Uint8Array([92, 178, 255]);
  gradient = new DataTexture(data, data.length, 1, RedFormat);
  gradient.minFilter = NearestFilter;
  gradient.magFilter = NearestFilter;
  gradient.generateMipmaps = false;
  gradient.needsUpdate = true;
  return gradient;
}

export interface ToonOpts {
  color?: string | number;
  vertexColors?: boolean;
  rim?: number; // strength of the fresnel rim light (characters/entities)
  rimColor?: string;
  emissive?: string | number;
  emissiveIntensity?: number;
}

const rimCache = new Map<string, MeshToonMaterial>();

/** Toon material with an optional soft rim light (for characters and props). Cached by options. */
export function toon(o: ToonOpts = {}): MeshToonMaterial {
  const key = JSON.stringify(o);
  const hit = rimCache.get(key);
  if (hit) return hit;
  const m = new MeshToonMaterial({ color: o.color ?? 0xffffff, gradientMap: toonGradient(), vertexColors: !!o.vertexColors });
  if (o.emissive !== undefined) { m.emissive = new Color(o.emissive); m.emissiveIntensity = o.emissiveIntensity ?? 1; }
  const rim = o.rim ?? 0;
  if (rim > 0) {
    const rimColor = new Color(o.rimColor ?? '#fff4e8');
    m.onBeforeCompile = (sh) => {
      sh.uniforms.uRim = { value: rim };
      sh.uniforms.uRimColor = { value: rimColor };
      sh.fragmentShader = sh.fragmentShader
        .replace('#include <common>', '#include <common>\nuniform float uRim; uniform vec3 uRimColor;')
        .replace('#include <opaque_fragment>', /* glsl */ `
          float rimF = 1.0 - saturate(dot(normal, normalize(vViewPosition)));
          outgoingLight += uRimColor * smoothstep(0.55, 0.75, rimF) * uRim;
          #include <opaque_fragment>`);
    };
    m.customProgramCacheKey = () => 'rim';
  }
  rimCache.set(key, m);
  return m;
}

/**
 * Material for the merged static world. Per-vertex attributes:
 *   aBary (vec3) barycentric coords with internal (diagonal) edges disabled → crisp baked edge lines
 *   aKind (float) 0 ground, 1 building, 2 rock, 3 wall, 4 prop
 * Adds world-space patterns: speed grid on the ground, ribbon windows on buildings.
 */
export function staticsMaterial(): MeshToonMaterial {
  const m = new MeshToonMaterial({ color: 0xffffff, gradientMap: toonGradient(), vertexColors: true });
  const ink = new Color(INK);
  m.onBeforeCompile = (sh) => {
    sh.uniforms.uInk = { value: ink };
    sh.vertexShader = sh.vertexShader
      .replace('#include <common>', `#include <common>
        attribute vec3 aBary; attribute float aKind;
        varying vec3 vBary; varying float vKind; varying vec3 vWPos; varying vec3 vWNormal;`)
      .replace('#include <begin_vertex>', `#include <begin_vertex>
        vBary = aBary; vKind = aKind;`)
      .replace('#include <project_vertex>', `#include <project_vertex>
        vWPos = (modelMatrix * vec4(transformed, 1.0)).xyz;
        vWNormal = normalize(mat3(modelMatrix) * objectNormal);`);
    sh.fragmentShader = sh.fragmentShader
      .replace('#include <common>', `#include <common>
        uniform vec3 uInk;
        varying vec3 vBary; varying float vKind; varying vec3 vWPos; varying vec3 vWNormal;
        float gridLine(float x, float period, float widthPx) {
          float d = abs(fract(x / period + 0.5) - 0.5) * period;
          float fw = max(fwidth(x), 1e-4);
          return 1.0 - smoothstep(fw * (widthPx - 0.5), fw * (widthPx + 0.5), d);
        }
        float hashS(vec2 p) { vec3 p3 = fract(vec3(p.xyx) * 0.1031); p3 += dot(p3, p3.yzx + 33.33); return fract((p3.x + p3.y) * p3.z); }
        float band(float x, float a, float b) {
          float fw = max(fwidth(x), 1e-4) * 0.75;
          return smoothstep(a - fw, a + fw, x) * (1.0 - smoothstep(b - fw, b + fw, x));
        }`)
      .replace('#include <color_fragment>', /* glsl */ `#include <color_fragment>
        {
          float dist = length(vWPos - cameraPosition);
          vec3 n = normalize(vWNormal);
          if (vKind < 0.5) {
            // ground: soft tile variation + speed grid (minor 4 m, major 32 m)
            vec2 cell = floor(vWPos.xz / 16.0);
            float tile = hashS(cell);
            diffuseColor.rgb *= 0.94 + 0.08 * tile;
            float minor = gridLine(vWPos.x, 4.0, 0.9) + gridLine(vWPos.z, 4.0, 0.9);
            float major = gridLine(vWPos.x, 32.0, 1.6) + gridLine(vWPos.z, 32.0, 1.6);
            float fadeMinor = 1.0 - smoothstep(40.0, 140.0, dist);
            float fadeMajor = 1.0 - smoothstep(200.0, 600.0, dist);
            diffuseColor.rgb = mix(diffuseColor.rgb, diffuseColor.rgb * 1.16 + 0.03, clamp(minor, 0.0, 1.0) * 0.55 * fadeMinor);
            diffuseColor.rgb = mix(diffuseColor.rgb, vec3(0.96, 0.95, 0.9), clamp(major, 0.0, 1.0) * 0.45 * fadeMajor);
          } else if (vKind < 1.5) {
            // buildings: ribbon windows on walls, slightly darker roofs, darker base
            if (abs(n.y) < 0.3) {
              vec2 t = normalize(vec2(-n.z, n.x));
              float u = dot(vWPos.xz, t);
              float v = vWPos.y;
              float floorH = 3.6;
              float fv = fract(v / floorH);
              float win = band(fv, 0.4, 0.76) * step(4.0, v);
              float mull = gridLine(u, 3.2, 0.8);
              float lod = 1.0 - smoothstep(60.0, 260.0, dist);
              vec3 glass = mix(vec3(0.46, 0.58, 0.78), vec3(0.78, 0.86, 0.96), fv);
              diffuseColor.rgb = mix(diffuseColor.rgb, mix(diffuseColor.rgb, glass, 0.5), win * (1.0 - mull * 0.85) * mix(0.25, 1.0, lod));
              diffuseColor.rgb *= 0.8 + 0.2 * smoothstep(0.0, 3.0, v);
            } else if (n.y > 0.7) {
              diffuseColor.rgb *= 0.9;
            }
          } else if (vKind < 2.5) {
            // rock / ramps: gentle diagonal stripes
            float s = band(fract((vWPos.x + vWPos.z + vWPos.y) / 2.5), 0.0, 0.5);
            diffuseColor.rgb *= 0.97 + 0.05 * s;
          } else if (vKind < 3.5) {
            // walls: horizontal courses
            diffuseColor.rgb *= 0.95 + 0.06 * gridLine(vWPos.y, 1.2, 0.8);
          }
        }`)
      .replace('#include <opaque_fragment>', /* glsl */ `
        {
          vec3 fw = fwidth(vBary);
          vec3 a = smoothstep(fw * 0.6, fw * 1.7, vBary);
          float edge = 1.0 - min(min(a.x, a.y), a.z);
          float d = length(vWPos - cameraPosition);
          edge *= 1.0 - smoothstep(180.0, 520.0, d);
          outgoingLight = mix(outgoingLight, mix(outgoingLight, uInk, 0.8), edge);
        }
        #include <opaque_fragment>`);
  };
  m.customProgramCacheKey = () => 'hl-statics';
  return m;
}

export function disposeMaterial(m: Material | Material[]) {
  if (Array.isArray(m)) m.forEach((x) => x.dispose()); else m.dispose();
}
