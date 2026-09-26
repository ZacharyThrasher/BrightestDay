#!/usr/bin/env node
// System-2 feasibility bench: small instruct LLM via llama.cpp's llama-server (CPU here).
//   (a) option-logit scoring: for each typed question, ONE forward pass over the question suffix
//       (the system prompt + utterance prefix is KV-cached) and read the next-token distribution;
//       the answer is the option whose first token(s) carry the most probability. No generation.
//       Also reports "calibrated" accuracy: option probs divided by the probs for a content-free
//       command ("N/A"), computed once per question (contextual calibration, Zhao et al. 2021).
//   (b) short constrained JSON generation (llama-server `json_schema` -> GBNF grammar).
//
//   node tools/semantic/bench_llm.mjs [--model qwen2.5-1.5b-instruct-q4_k_m.gguf] [--threads 2] [--fewshot] [--json out.json]
//   (--threads 2 on the 4-vCPU dev VM: 3-4 threads collapse there because of vCPU oversubscription)
// Needs models/semantic/raw/x/llama-b10951/llama-server (download.mjs --group llm, then untar).
import { spawn } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const raw = join(repo, 'models', 'semantic', 'raw');
const args = process.argv.slice(2);
const opt = (k, d) => (args.includes(k) ? args[args.indexOf(k) + 1] : d);
const model = opt('--model', 'qwen2.5-1.5b-instruct-q4_k_m.gguf');
const threads = opt('--threads', '2');
const port = Number(opt('--port', 8812));
const server = opt('--server', join(raw, 'x', 'llama-b10951', 'llama-server'));
const base = `http://127.0.0.1:${port}`;

// ---------------------------------------------------------------- typed questions + gold labels
const Q = {
  intent: ['What does the player want? create = make a new object; modify = change an existing object; act = make an existing object do something; control = game command like undo/delete/pause.', ['create', 'modify', 'act', 'control']],
  target: ['Which existing thing does the command refer to? self = the player; aimed = something the player points at ("that X", "the X"); previous = the last thing made ("it"); none = nothing existing.', ['self', 'aimed', 'previous', 'none']],
  compound: ['Does the command ask for two or more separate steps?', ['yes', 'no']],
  size: ['What size is requested?', ['small', 'normal', 'large']],
  attach: ['Does the command attach one part onto another part?', ['yes', 'no']],
  count: ['How many new objects?', ['one', 'several', 'zero']],
  negation: ['Does the command remove, stop or undo something?', ['yes', 'no']],
  motion: ['What motion is requested?', ['none', 'spin', 'launch', 'swing', 'punch']],
  surface: ['What surface property is requested?', ['normal', 'bouncy', 'slippery', 'sticky']],
};
// gold: accepted answers per question (arrays = several acceptable)
const U = [
  ['make a giant boxing glove with rockets on the back and punch that drone', { intent: 'create', target: 'aimed', compound: 'yes', size: 'large', attach: 'yes', count: 'one', negation: 'no', motion: 'punch', surface: 'normal' }],
  ['build a ramp in front of me', { intent: 'create', target: ['self', 'none'], compound: 'no', size: 'normal', attach: 'no', count: 'one', negation: 'no', motion: 'none', surface: 'normal' }],
  ['make it bigger', { intent: 'modify', target: 'previous', compound: 'no', size: 'large', attach: 'no', count: 'zero', negation: 'no', motion: 'none', surface: 'normal' }],
  ['spin that wheel really fast', { intent: 'act', target: 'aimed', compound: 'no', size: 'normal', attach: 'no', count: 'zero', negation: 'no', motion: 'spin', surface: 'normal' }],
  ['undo that', { intent: 'control', target: ['previous', 'aimed'], compound: 'no', size: 'normal', attach: 'no', count: 'zero', negation: 'yes', motion: 'none', surface: 'normal' }],
  ['throw three bouncy balls at the tower', { intent: 'create', target: 'aimed', compound: 'yes', size: 'normal', attach: 'no', count: 'several', negation: 'no', motion: 'launch', surface: 'bouncy' }],
  ['attach a tiny magnet to it', { intent: 'create', target: 'previous', compound: 'no', size: 'small', attach: 'yes', count: 'one', negation: 'no', motion: 'none', surface: 'normal' }],
  ['delete everything', { intent: 'control', target: 'none', compound: 'no', size: 'normal', attach: 'no', count: 'zero', negation: 'yes', motion: 'none', surface: 'normal' }],
  ['make the bridge slippery', { intent: 'modify', target: 'aimed', compound: 'no', size: 'normal', attach: 'no', count: 'zero', negation: 'no', motion: 'none', surface: 'slippery' }],
  ['swing the hammer at the wall', { intent: 'act', target: 'aimed', compound: 'no', size: 'normal', attach: 'no', count: 'zero', negation: 'no', motion: 'swing', surface: 'normal' }],
  ['give me a sticky wall twice as tall as me', { intent: 'create', target: ['self', 'none'], compound: 'no', size: 'large', attach: 'no', count: 'one', negation: 'no', motion: 'none', surface: 'sticky' }],
  ['launch me into the air with a trampoline', { intent: 'create', target: 'self', compound: 'yes', size: 'normal', attach: 'no', count: 'one', negation: 'no', motion: 'launch', surface: 'bouncy' }],
];

const FEWSHOT = `
Definitions: create = make a new object; modify = change an existing object; act = make an existing object do something; control = undo/delete/pause.
target: self = the player ("me"); aimed = an existing thing named or pointed at ("that X", "the X"); previous = the last thing made ("it"); none.
Examples (question: answer):
Command: "make a huge spring under me" -> intent: create, target: self, compound: no, size: large, attach: no, count: one, negation: no, motion: none, surface: bouncy
Command: "shrink it" -> intent: modify, target: previous, compound: no, size: small, attach: no, count: zero, negation: no, motion: none, surface: normal
Command: "stick a rocket on that cart and fire it" -> intent: create, target: aimed, compound: yes, size: normal, attach: yes, count: one, negation: no, motion: launch, surface: normal
Command: "remove the cage" -> intent: control, target: aimed, compound: no, size: normal, attach: no, count: zero, negation: yes, motion: none, surface: normal
Command: "rotate the turntable" -> intent: act, target: aimed, compound: no, size: normal, attach: no, count: zero, negation: no, motion: spin, surface: normal`;
const SYS = 'You label voice commands for a physics sandbox game where the player creates glowing hard-light objects. Reply with exactly one word from the given options.'
  + (args.includes('--fewshot') ? FEWSHOT : '');
const chat = (user) => `<|im_start|>system\n${SYS}<|im_end|>\n<|im_start|>user\n${user}`;
const prefix = (utt) => chat(`Command: "${utt}"\n`);
const questionSuffix = (q) => `Question: ${Q[q][0]}\nOptions: ${Q[q][1].join(', ')}<|im_end|>\n<|im_start|>assistant\n`;

async function post(path, body) {
  const r = await fetch(base + path, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) });
  if (!r.ok) throw new Error(`${path}: HTTP ${r.status} ${await r.text()}`);
  return r.json();
}

async function main() {
  const proc = spawn(server, ['-m', join(raw, model), '-t', threads, '-c', '4096', '--port', String(port), '-np', '1', '--no-webui'],
    { stdio: ['ignore', 'ignore', 'pipe'] });
  let log = '';
  proc.stderr.on('data', (d) => { log += d; });
  try {
    for (let i = 0; ; i++) {
      try { if ((await fetch(base + '/health')).ok) break; } catch {}
      if (i > 120) throw new Error('server did not start\n' + log.slice(-2000));
      await new Promise((r) => setTimeout(r, 500));
    }
    // first-token ids for every option (lower/Capitalised, with/without leading space)
    const firstTok = {};
    for (const [, opts] of Object.values(Q)) for (const o of opts) {
      if (firstTok[o]) continue;
      const ids = new Set();
      for (const v of [o, ' ' + o, o[0].toUpperCase() + o.slice(1), ' ' + o[0].toUpperCase() + o.slice(1)]) {
        ids.add((await post('/tokenize', { content: v })).tokens[0]);
      }
      firstTok[o] = ids;
    }
    for (const [q, [, opts]] of Object.entries(Q)) {
      const seen = new Map();
      for (const o of opts) for (const id of firstTok[o]) {
        if (seen.has(id)) console.warn(`WARN first-token collision in ${q}: ${o} vs ${seen.get(id)}`);
        seen.set(id, o);
      }
    }
    await post('/completion', { prompt: prefix('warm up') + questionSuffix('intent'), n_predict: 1, cache_prompt: true });

    const optionProbs = async (utt, q) => {
      const r = await post('/completion', { prompt: prefix(utt) + questionSuffix(q), n_predict: 1, n_probs: 100, temperature: 0,
        cache_prompt: true, post_sampling_probs: false });
      const top = r.completion_probabilities[0].top_logprobs;
      return { r, score: Object.fromEntries(Q[q][1].map((o) => [o, top.filter((x) => firstTok[o].has(x.id)).reduce((s, x) => s + Math.exp(x.logprob), 0) + 1e-6])) };
    };
    // content-free prior per question (contextual calibration, Zhao et al. 2021): computed once, reused
    const prior = {};
    for (const q of Object.keys(Q)) prior[q] = (await optionProbs('N/A', q)).score;

    // (a) option-logit scoring
    let correct = 0, total = 0, correctCal = 0;
    const perQCal = {};
    const perQ = {}, lat = [], prefixLat = [], rows = [], promptN = [], serverMs = [];
    for (const [utt, gold] of U) {
      // prime the KV cache with the shared prefix (counted separately: it is paid once per utterance)
      let t = performance.now();
      await post('/completion', { prompt: prefix(utt), n_predict: 0, cache_prompt: true });
      prefixLat.push(performance.now() - t);
      const answers = {};
      for (const q of Object.keys(Q)) {
        t = performance.now();
        const { r, score } = await optionProbs(utt, q);
        lat.push(performance.now() - t);
        promptN.push(r.timings.prompt_n); serverMs.push(r.timings.prompt_ms + r.timings.predicted_ms);
        const best = Object.entries(score).sort((a, b) => b[1] - a[1])[0];
        const ok = [gold[q]].flat().includes(best[0]);
        const cal = Object.entries(score).map(([o, p]) => [o, p / prior[q][o]]).sort((a, b) => b[1] - a[1])[0];
        const okCal = [gold[q]].flat().includes(cal[0]);
        correctCal += okCal;
        perQCal[q] = perQCal[q] || [0, 0]; perQCal[q][0] += okCal; perQCal[q][1]++;
        answers[q] = `${best[0]}${ok ? '' : ' (gold ' + [gold[q]].flat().join('|') + ')'} p=${best[1].toFixed(2)} | cal:${cal[0]}${okCal ? '' : '(x)'}`;
        perQ[q] = perQ[q] || [0, 0];
        perQ[q][0] += ok; perQ[q][1]++;
        correct += ok; total++;
      }
      rows.push({ utt, answers });
    }
    const stat = (a) => { const s = [...a].sort((x, y) => x - y); return { mean: s.reduce((x, y) => x + y, 0) / s.length, p50: s[Math.floor(s.length / 2)], p95: s[Math.floor(s.length * 0.95)] }; };

    // (b) constrained JSON generation
    const schema = { type: 'object', additionalProperties: false, required: ['intent', 'object', 'size', 'target'],
      properties: { intent: { enum: ['create', 'modify', 'act', 'control'] }, object: { type: 'string', maxLength: 24 },
        size: { enum: ['small', 'normal', 'large'] }, target: { enum: ['self', 'aimed', 'previous', 'none'] } } };
    const jsonRows = [], jlat = [], jtoks = [];
    for (const [utt] of U) {
      const t = performance.now();
      const r = await post('/completion', {
        prompt: chat(`Command: "${utt}"\nReturn JSON with intent (create|modify|act|control), object (main object noun), size (small|normal|large), target (self|aimed|previous|none).<|im_end|>\n<|im_start|>assistant\n`),
        n_predict: 64, temperature: 0, json_schema: schema, cache_prompt: true });
      jlat.push(performance.now() - t); jtoks.push(r.tokens_predicted);
      let parsed = null; try { parsed = JSON.parse(r.content); } catch {}
      jsonRows.push({ utt, out: r.content, valid: !!parsed, prompt_ms: r.timings.prompt_ms, gen_ms: r.timings.predicted_ms, gen_tok_s: r.timings.predicted_per_second });
    }
    const res = {
      model, threads: Number(threads), fewshot: args.includes('--fewshot'), server: 'llama.cpp b10951 llama-server (CPU)',
      optionScoring: { accuracy: correct / total, calibratedAccuracy: correctCal / total, n: total,
        perQuestionCalibrated: Object.fromEntries(Object.entries(perQCal).map(([k, [c, n]]) => [k, `${c}/${n}`])), perQuestion: Object.fromEntries(Object.entries(perQ).map(([k, [c, n]]) => [k, `${c}/${n}`])),
        perQuestionMs: stat(lat), serverMs: stat(serverMs), promptTokensEvaluated: stat(promptN), prefixPrimeMs: stat(prefixLat),
        rawLatencies: lat.map((x, i) => [Math.round(x), promptN[i]]), rows },
      json: { validRate: jsonRows.filter((r) => r.valid).length / jsonRows.length, latencyMs: stat(jlat), meanTokens: jtoks.reduce((a, b) => a + b, 0) / jtoks.length,
        meanGenTokPerS: jsonRows.reduce((a, r) => a + r.gen_tok_s, 0) / jsonRows.length, rows: jsonRows },
    };
    console.log(JSON.stringify({ model, optionAccuracy: res.optionScoring.accuracy, calibratedAccuracy: res.optionScoring.calibratedAccuracy,
      perQuestion: res.optionScoring.perQuestion, perQuestionCalibrated: res.optionScoring.perQuestionCalibrated,
      perQuestionMs: res.optionScoring.perQuestionMs, serverMs: res.optionScoring.serverMs, promptTokensEvaluated: res.optionScoring.promptTokensEvaluated, prefixPrimeMs: res.optionScoring.prefixPrimeMs,
      json: { validRate: res.json.validRate, latencyMs: res.json.latencyMs, meanTokens: res.json.meanTokens, genTokPerS: res.json.meanGenTokPerS } }, null, 1));
    for (const r of rows) console.log(r.utt, JSON.stringify(r.answers));
    for (const r of jsonRows) console.log(r.utt, '=>', r.out);
    if (opt('--json')) writeFileSync(opt('--json'), JSON.stringify(res, null, 1));
  } finally {
    proc.kill();
  }
}
main().catch((e) => { console.error(e); process.exit(1); });
