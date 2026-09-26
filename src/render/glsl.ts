/** Shared GLSL snippets (sky model, noise). Kept as strings so every pass uses identical maths. */

export const NOISE_GLSL = /* glsl */ `
float hash12(vec2 p) { vec3 p3 = fract(vec3(p.xyx) * 0.1031); p3 += dot(p3, p3.yzx + 33.33); return fract((p3.x + p3.y) * p3.z); }
float hash13(vec3 p3) { p3 = fract(p3 * 0.1031); p3 += dot(p3, p3.zyx + 31.32); return fract((p3.x + p3.y) * p3.z); }
float vnoise(vec2 p) {
  vec2 i = floor(p); vec2 f = fract(p); vec2 u = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash12(i), hash12(i + vec2(1, 0)), u.x), mix(hash12(i + vec2(0, 1)), hash12(i + vec2(1, 1)), u.x), u.y);
}
float vnoise3(vec3 p) {
  vec3 i = floor(p); vec3 f = fract(p); vec3 u = f * f * (3.0 - 2.0 * f);
  float a = mix(mix(hash13(i), hash13(i + vec3(1,0,0)), u.x), mix(hash13(i + vec3(0,1,0)), hash13(i + vec3(1,1,0)), u.x), u.y);
  float b = mix(mix(hash13(i + vec3(0,0,1)), hash13(i + vec3(1,0,1)), u.x), mix(hash13(i + vec3(0,1,1)), hash13(i + vec3(1,1,1)), u.x), u.y);
  return mix(a, b, u.z);
}
`;

/** Sky gradient + sun + optional cel clouds. Uniforms: uSkyTop, uSkyHorizon, uSkyGround, uSunDir, uSunColor, uTime, uClouds. */
export const SKY_GLSL = /* glsl */ `
uniform vec3 uSkyTop;
uniform vec3 uSkyHorizon;
uniform vec3 uSkyGround;
uniform vec3 uSunDir;
uniform vec3 uSunColor;
uniform float uClouds;
${NOISE_GLSL}
vec3 skyGradient(vec3 dir) {
  float h = dir.y;
  vec3 c;
  if (h >= 0.0) c = mix(uSkyHorizon, uSkyTop, pow(clamp(h, 0.0, 1.0), 0.5));
  else c = mix(uSkyHorizon, uSkyGround, pow(clamp(-h * 3.0, 0.0, 1.0), 0.6));
  float sd = max(dot(dir, uSunDir), 0.0);
  c += uSunColor * pow(sd, 6.0) * 0.18; // warm halo around the sun
  return c;
}
vec3 skyColor(vec3 dir, float time) {
  vec3 c = skyGradient(dir);
  float sd = max(dot(dir, uSunDir), 0.0);
  c += uSunColor * smoothstep(0.9993, 0.9996, sd) * 3.0; // crisp HDR sun disc (blooms)
  if (uClouds > 0.5 && dir.y > 0.02) {
    // flat cel clouds on a virtual plane: two tones, hard-edged
    vec2 uv = dir.xz / (dir.y + 0.12) * 1.3 + vec2(time * 0.004, time * 0.0015);
    float n = vnoise(uv * 1.1) * 0.55 + vnoise(uv * 2.3 + 7.1) * 0.3 + vnoise(uv * 5.1 + 3.3) * 0.15;
    float fade = smoothstep(0.02, 0.25, dir.y) * (1.0 - smoothstep(0.55, 0.95, dir.y));
    float body = smoothstep(0.60, 0.63, n) * fade;
    float core = smoothstep(0.70, 0.73, n) * fade;
    vec3 cloudShade = mix(uSkyHorizon, vec3(1.0), 0.55);
    c = mix(c, cloudShade, body * 0.75);
    c = mix(c, vec3(1.0, 0.99, 0.97), core * 0.8);
  }
  return c;
}
`;
