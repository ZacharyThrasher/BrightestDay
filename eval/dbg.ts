// Debug a single eval item: npx tsx eval/dbg.ts <file> <id>
import { readFileSync } from 'node:fs';
import { runItem } from './run.js';
const [file, id] = process.argv.slice(2);
const it = readFileSync(file, 'utf8').split('\n').filter(Boolean).map((l) => JSON.parse(l)).find((x) => x.id === id);
const r = await runItem(it);
console.log(JSON.stringify({ ctx: it.context, utt: it.utterance, pass: r.pass, reasons: r.reasons, summary: r.summary, fns: r.report.functions, intents: (r.report as any).intents, targets: (r.report as any).targetKinds, msgs: r.messages }, null, 1));
