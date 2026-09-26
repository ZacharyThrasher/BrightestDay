// Real-click UI smoke test of the Electron build (run after `npm run build`):
//   xvfb-run -a node tools/ui-smoke.mjs
// Checks the things a player hits in the first 10 seconds: Play captures the mouse and hides the menu,
// Enter opens the console, a typed command forms a construct.
import { _electron as electron } from 'playwright-core';
import { createRequire } from 'node:module';
const electronPath = createRequire(import.meta.url)('electron');
const app = await electron.launch({ executablePath: electronPath, args: ['.', '--no-sandbox', '--software-gl'] });
const win = await app.firstWindow();
const fail = async (msg) => { console.error('UI SMOKE FAIL:', msg); await app.close(); process.exit(1); };
const state = () => win.evaluate(() => ({ locked: !!document.pointerLockElement, menuHidden: document.querySelector('.menu')?.classList.contains('hidden') }));
await win.waitForFunction(() => document.body.classList.contains('ready'), null, { timeout: 45000 });
await win.click('button.play');
await win.waitForTimeout(600);
let s = await state();
if (!s.locked || !s.menuHidden) await fail(`after Play: ${JSON.stringify(s)}`);
await win.keyboard.press('Enter');
await win.waitForTimeout(200);
await win.keyboard.type('a wall');
await win.keyboard.press('Enter');
await win.waitForFunction(() => window.hardlightApp.world.constructs.list.size > 0, null, { timeout: 5000 }).catch(() => fail('typed "a wall" formed nothing'));
console.log('UI smoke OK: play locks the mouse, menu hides, typed command forms a construct');
await app.close();
