// quick dev screenshot: node tools/shot-quick.mjs out.png [scene]
import { launch, frames } from './render-harness.mjs';
const out = process.argv[2] ?? 'docs/screenshots/_quick.png';
const { page, errors, close } = await launch({ query: process.argv[3] ? `scene=${process.argv[3]}` : '' });
await frames(page, 4);
await page.screenshot({ path: out });
console.log(errors.join('\n') || 'no errors');
console.log(await page.evaluate(() => JSON.stringify(window.__hl.stats())));
await close();
