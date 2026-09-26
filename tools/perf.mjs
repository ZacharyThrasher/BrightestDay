#!/usr/bin/env node
// Headless perf harness (Chromium + SwiftShader). Measures per-frame CPU work (sim step, render
// scene-graph update + GL submission, HUD), rAF frame time, draw calls and triangles for three
// scenarios, at graphics=high and graphics=low. Writes docs/experiments/RENDER_PERF.md.
//
// CAVEAT: SwiftShader is a CPU software rasteriser. Frame times here are dominated by software
// pixel work and are NOT representative of a GPU (e.g. RTX 3080). The meaningful numbers are the
// CPU-side ms (sim + JS + scene graph) and the draw-call / triangle budgets.
//   node tools/perf.mjs [--frames=240] [--size=1280x720] [--quality=high,low]
import { launch } from './render-harness.mjs';
import { writeFileSync, mkdirSync } from 'node:fs';
import os from 'node:os';

const args = Object.fromEntries(process.argv.slice(2).map((a) => a.replace(/^--/, '').split('=')));
const N = Number(args.frames ?? 240);
const [W, H] = (args.size ?? '1280x720').split('x').map(Number);
const qualities = (args.quality ?? 'high,low').split(',');

const pct = (arr, p) => { const s = [...arr].sort((a, b) => a - b); return s.length ? s[Math.min(s.length - 1, Math.floor((p / 100) * s.length))] : 0; };
const mean = (a) => a.reduce((x, y) => x + y, 0) / Math.max(1, a.length);

const SCENARIOS = [
  {
    name: 'idle playground',
    setup: async (page) => { await page.evaluate(() => { window.__hl.look(0, -0.05); window.__hl.step(30); }); },
    input: `return {};`,
  },
  {
    name: 'high-speed traversal',
    // boost-flying down a city avenue at ~40-60 m/s with speed FX active
    setup: async (page) => { await page.evaluate(() => { const H = window.__hl; H.teleport([0, 25, 20], [0, 0, -45]); H.look(0, -0.05); H.step(1, { flyToggle: true }); }); },
    input: `const P = w.player; if (P.pos.z < -420) { P.teleport(P.pos.clone().setZ(20), P.vel.clone()); } return { moveZ: 1, boost: true, yaw: Math.sin(i * 0.02) * 0.15, pitch: -0.05 };`,
  },
  {
    name: 'stress: 20 constructs + projectiles',
    setup: async (page) => {
      await page.evaluate(() => {
        const H = window.__hl;
        H.look(0.3, -0.12);
        H.step(10);
        const forms = ['wall', 'ramp', 'glove', 'cage', 'dome', 'trampoline', 'hammer', 'shield', 'bridge', 'fan', 'gear', 'car', 'dragon', 'saw', 'spring', 'chain', 'tornado', 'rocket', 'helicopter', 'hand'];
        forms.forEach((f, k) => {
          const a = (k / forms.length) * Math.PI * 2;
          const r = 14 + (k % 3) * 6;
          H.spawnForm(f, { scale: 1.2, lifetime: 140, placement: { at: 'target', target: { kind: 'point', p: [Math.sin(a) * r, 3 + (k % 4), -Math.cos(a) * r - 10] }, orient: 'face_away' }, behaviors: k % 4 === 0 ? [{ type: 'spin', rate: 1.5, axis: [0, 1, 0] }] : [] });
        });
      });
    },
    input: `if (i % 8 === 0) window.__hl.fire(12, 24); return {};`,
  },
];

const results = [];
for (const q of qualities) {
  const { page, errors, close } = await launch({ width: W, height: H, query: `graphics=${q}` });
  for (const sc of SCENARIOS) {
    await page.evaluate(() => window.__hl.scene('playground'));
    await sc.setup(page);
    await page.evaluate(() => window.__hl.frames(5));
    const samples = await page.evaluate(([n, src]) => window.__hl.bench(n, 1, src), [N, sc.input]);
    const st = await page.evaluate(() => window.__hl.stats());
    const cpu = samples.map((s) => s.sim + s.render + s.hud);
    const r = {
      quality: q, scenario: sc.name, frames: samples.length,
      frame: { p50: pct(samples.map((s) => s.frame), 50), p95: pct(samples.map((s) => s.frame), 95), p99: pct(samples.map((s) => s.frame), 99) },
      sim: { mean: mean(samples.map((s) => s.sim)), p95: pct(samples.map((s) => s.sim), 95) },
      render: { mean: mean(samples.map((s) => s.render)), p95: pct(samples.map((s) => s.render), 95), update: mean(samples.map((s) => s.update)) },
      hud: mean(samples.map((s) => s.hud)),
      cpu: { p50: pct(cpu, 50), p95: pct(cpu, 95), p99: pct(cpu, 99) },
      calls: { mean: mean(samples.map((s) => s.calls)), max: Math.max(...samples.map((s) => s.calls)), total: Math.max(...samples.map((s) => s.total)) },
      tris: { mean: mean(samples.map((s) => s.tris)), max: Math.max(...samples.map((s) => s.tris)) },
      extra: { constructs: st.constructs, projectiles: st.projectiles, particles: st.particles, beams: st.beams, entities: st.entities },
    };
    results.push(r);
    console.log(`[${q}] ${sc.name.padEnd(36)} cpu p50 ${r.cpu.p50.toFixed(2)} p95 ${r.cpu.p95.toFixed(2)} ms | sim ${r.sim.mean.toFixed(2)} | render ${r.render.mean.toFixed(2)} (upd ${r.render.update.toFixed(2)}) | hud ${r.hud.toFixed(2)} | frame p50 ${r.frame.p50.toFixed(0)} ms | calls ${r.calls.mean.toFixed(0)} (max ${r.calls.max}, +post ${r.calls.total - r.calls.max}) | tris ${Math.round(r.tris.mean)}`);
  }
  if (errors.length) console.log('page errors:', [...new Set(errors)].join('\n'));
  await close();
}

const f = (x, d = 2) => x.toFixed(d);
let md = `# Renderer performance (headless, SwiftShader)

Generated by \`node tools/perf.mjs\` on ${new Date().toISOString().slice(0, 10)} — ${os.cpus().length} × ${os.cpus()[0]?.model ?? 'cpu'}, no GPU, ${W}×${H}, ${N} frames per scenario.

> **Caveat — read before quoting numbers.** Chromium here renders with **SwiftShader**, a CPU
> software rasteriser. rAF frame times below are dominated by software pixel shading (MSAA, bloom,
> shadow map, full-screen passes on the CPU) and say nothing about how the game runs on a real GPU
> such as an RTX 3080. The meaningful numbers are:
> - **CPU ms per frame** = sim step + renderer JS (scene sync, interpolation, particle/beam update,
>   Three.js scene traversal and WebGL command submission) + HUD DOM update. This work is the same
>   on a gaming PC (a desktop CPU is typically 1.5–3× faster than this container's vCPUs).
> - **Draw calls / triangles** per frame (scene pass incl. shadow map; "+post" = full-screen passes).
>
> Budget targets: CPU < 4 ms/frame and < 300 draw calls in normal play.
> Note: WebGL calls return before the (software) GPU finishes, but SwiftShader shares the same CPU
> cores and can back-pressure the command buffer, so CPU ms here are, if anything, pessimistic.

| quality | scenario | CPU/frame p50 | p95 | p99 | sim/tick | render JS (of which update) | HUD | draw calls (max) | +post | triangles | rAF frame p50 / p95 / p99 (SwiftShader) |
|---|---|---|---|---|---|---|---|---|---|---|---|
`;
for (const r of results) {
  md += `| ${r.quality} | ${r.scenario} | ${f(r.cpu.p50)} ms | ${f(r.cpu.p95)} | ${f(r.cpu.p99)} | ${f(r.sim.mean)} ms | ${f(r.render.mean)} (${f(r.render.update)}) ms | ${f(r.hud)} ms | ${f(r.calls.mean, 0)} (${r.calls.max}) | ${r.calls.total - r.calls.max} | ${Math.round(r.tris.mean).toLocaleString('en-US')} | ${f(r.frame.p50, 0)} / ${f(r.frame.p95, 0)} / ${f(r.frame.p99, 0)} ms |\n`;
}
md += `
Scenario details (final frame): ${results.filter((r) => r.quality === qualities[0]).map((r) => `**${r.scenario}** — ${r.extra.constructs} construct views, ${r.extra.projectiles} projectiles, ${r.extra.particles} particles, ${r.extra.beams} beams, ${r.extra.entities} entities`).join('; ')}.

## How to read / reproduce

- \`npm run build && node tools/perf.mjs\` (≈ a few minutes; \`--frames=N --size=WxH --quality=high,low\`).
- The sim is stepped exactly one 60 Hz tick per rendered frame inside the page (\`__hl.bench\`), so
  "CPU/frame" is the cost of one tick + one frame. At 120/144 Hz display rates most frames run
  zero sim ticks (render-only, interpolated), so per-frame CPU is lower than shown.
- "high" = MSAA 4× HDR target, 2048² PCF shadow map, 5-level bloom, clouds; "low" = no MSAA,
  no shadow map (drop-marker only), 4-level bloom, no clouds, pixel ratio capped at 1.
`;
mkdirSync('docs/experiments', { recursive: true });
writeFileSync('docs/experiments/RENDER_PERF.md', md);
writeFileSync('docs/experiments/render_perf.json', JSON.stringify({ size: [W, H], frames: N, results }, null, 2));
console.log('wrote docs/experiments/RENDER_PERF.md');
