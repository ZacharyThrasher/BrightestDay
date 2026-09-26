// usage: npx tsx tools/map-room.ts <scene> <half-size m>
// Measure open space in a scene: street widths and the largest open square (raycasts on a grid).
import { World } from '../src/sim/world.js';
import { buildScene } from '../src/sim/levels.js';
const w = await World.create(); buildScene(w, process.argv[2] as any); w.step();
const R = w.R; const half = Number(process.argv[3] ?? 250), step = 4;
let open = 0, total = 0; const grid: boolean[][] = [];
for (let x = -half; x <= half; x += step) { const row: boolean[] = []; for (let z = -half; z <= half; z += step) {
  const hit = w.pw.castRay(new R.Ray({ x, y: 200, z }, { x: 0, y: -1, z: 0 }), 400, true);
  const y = hit ? 200 - hit.timeOfImpact : 0; const free = y < 2; row.push(free); total++; if (free) open++; } grid.push(row); }
// largest open square (in cells) via DP
const n = grid.length; let best = 0; const dp = grid.map((r) => r.map(() => 0));
for (let i = 0; i < n; i++) for (let j = 0; j < n; j++) if (grid[i][j]) { dp[i][j] = i && j ? 1 + Math.min(dp[i-1][j], dp[i][j-1], dp[i-1][j-1]) : 1; best = Math.max(best, dp[i][j]); }
// narrowest street: min over street cells of open run length along x and z (approx: median of runs)
const runs: number[] = []; for (const row of grid) { let run = 0; for (const f of row) { if (f) run++; else { if (run) runs.push(run * step); run = 0; } } if (run) runs.push(run * step); }
runs.sort((a, b) => a - b);
console.log(`${process.argv[2]}: open ground ${(100 * open / total).toFixed(0)}%, largest open square ${(best * step)} m, open-run widths p10/p50 ${runs[Math.floor(runs.length * 0.1)]}/${runs[Math.floor(runs.length * 0.5)]} m, statics ${w.statics?.length ?? '?'}`);
