import {
  BufferGeometry, Float32BufferAttribute, Color, DirectionalLight, HemisphereLight, Matrix4, Mesh, Object3D,
  PerspectiveCamera, ShaderMaterial, Vector3, type Scene,
} from 'three';
import { LIGHT, SKY, QUALITY, type Quality } from './config.js';
import { SKY_GLSL } from './glsl.js';

export function skyUniforms() {
  const sun = new Vector3(...SKY.sunDir).normalize();
  return {
    uSkyTop: { value: new Color(SKY.zenith) },
    uSkyHorizon: { value: new Color(SKY.horizon) },
    uSkyGround: { value: new Color(SKY.ground) },
    uSunDir: { value: sun },
    uSunColor: { value: new Color(SKY.sun) },
    uClouds: { value: 1 },
    uTime: { value: 0 },
  };
}

/**
 * Sky (a full-screen triangle drawn behind everything, 1 draw call), sun + hemisphere light,
 * and a player-following shadow frustum.
 */
export class Environment {
  sky: Mesh;
  sun: DirectionalLight;
  hemi: HemisphereLight;
  uniforms = skyUniforms();
  private skyMat: ShaderMaterial;
  private invProj = new Matrix4();
  private shadowExtent = 70;

  constructor(scene: Scene, quality: Quality) {
    const q = QUALITY[quality];
    this.uniforms.uClouds.value = q.clouds ? 1 : 0;
    const geo = new BufferGeometry();
    geo.setAttribute('position', new Float32BufferAttribute([-1, -1, 0, 3, -1, 0, -1, 3, 0], 3));
    this.skyMat = new ShaderMaterial({
      uniforms: { ...this.uniforms, uInvProj: { value: this.invProj }, uCamWorld: { value: new Matrix4() } },
      vertexShader: /* glsl */ `
        uniform mat4 uInvProj; uniform mat4 uCamWorld;
        varying vec3 vDir;
        void main() {
          vec4 p = uInvProj * vec4(position.xy, 1.0, 1.0);
          vDir = (uCamWorld * vec4(p.xyz / p.w, 0.0)).xyz;
          gl_Position = vec4(position.xy, 1.0, 1.0);
        }`,
      fragmentShader: /* glsl */ `
        uniform float uTime;
        ${SKY_GLSL}
        varying vec3 vDir;
        void main() { gl_FragColor = vec4(skyColor(normalize(vDir), uTime), 1.0); }`,
      depthWrite: false,
      depthTest: false,
    });
    this.sky = new Mesh(geo, this.skyMat);
    this.sky.frustumCulled = false;
    this.sky.renderOrder = -1000;
    this.sky.name = 'sky';
    scene.add(this.sky);

    this.hemi = new HemisphereLight(new Color(LIGHT.hemiSky), new Color(LIGHT.hemiGround), LIGHT.hemiIntensity);
    scene.add(this.hemi);
    this.sun = new DirectionalLight(new Color(SKY.sun), LIGHT.sunIntensity);
    this.sun.castShadow = q.shadows;
    if (q.shadows) {
      const s = this.sun.shadow;
      s.mapSize.set(q.shadowMapSize, q.shadowMapSize);
      const e = this.shadowExtent;
      s.camera.left = -e; s.camera.right = e; s.camera.top = e; s.camera.bottom = -e;
      s.camera.near = 1; s.camera.far = 700;
      s.bias = -0.0006;
      s.normalBias = 0.04;
      s.radius = 2;
      s.camera.updateProjectionMatrix();
    }
    scene.add(this.sun);
    scene.add(this.sun.target as Object3D);
  }

  update(camera: PerspectiveCamera, focus: Vector3, time: number) {
    this.invProj.copy(camera.projectionMatrixInverse);
    this.skyMat.uniforms.uCamWorld.value.copy(camera.matrixWorld);
    this.uniforms.uTime.value = time;
    // shadow frustum follows the player, snapped to shadow texels to avoid shimmering
    const texel = (this.shadowExtent * 2) / this.sun.shadow.mapSize.x;
    const sunDir = this.uniforms.uSunDir.value;
    // snap in light space
    const lightRight = new Vector3().crossVectors(sunDir, new Vector3(0, 1, 0)).normalize();
    const lightUp = new Vector3().crossVectors(lightRight, sunDir).normalize();
    const r = Math.round(focus.dot(lightRight) / texel) * texel;
    const u = Math.round(focus.dot(lightUp) / texel) * texel;
    const d = focus.dot(sunDir);
    const snapped = new Vector3().addScaledVector(lightRight, r).addScaledVector(lightUp, u).addScaledVector(sunDir, d);
    this.sun.target.position.copy(snapped);
    this.sun.position.copy(snapped).addScaledVector(sunDir, 350);
    this.sun.target.updateMatrixWorld();
    this.sun.updateMatrixWorld();
  }
}
