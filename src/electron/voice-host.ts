/**
 * Speech recognition host: runs as an Electron utilityProcess (its own OS process, so decoding never
 * competes with the render thread). Receives 16 kHz mono Float32 PCM, emits partial/final transcripts.
 *
 * parent -> child: {type:'init', modelsRoot, preset, hotwords?} | {type:'pcm', data:Float32Array} | {type:'end'} | {type:'reset'}
 * child -> parent: {type:'ready', preset, loadMs} | {type:'partial', text, t} | {type:'final', text, reason, t, audioMs, decodeMs} | {type:'error', message}
 */
import { join } from 'node:path';
import { existsSync } from 'node:fs';

declare const process: NodeJS.Process & { parentPort: { on(ev: 'message', cb: (e: { data: any }) => void): void; postMessage(m: any): void } };

const port = process.parentPort;
let engine: any = null;
let speechStart = 0;
let lastAudioAt = 0;
let audioSamples = 0;

function post(m: any) { port.postMessage(m); }

async function init(msg: { modelsRoot: string; preset?: string; hotwords?: string[]; threads?: number }) {
  const t0 = Date.now();
  try {
    // dynamic imports: the engine and presets are plain ESM next to this bundle
    const eng = await import(pathToUrl(join(__dirname, 'voice', 'asr-engine.mjs')));
    const pre = await import(pathToUrl(join(__dirname, 'voice', 'asr-presets.mjs')));
    const presetName = msg.preset && pre.PRESETS[msg.preset] ? msg.preset : pre.DEFAULT_PRESET ?? Object.keys(pre.PRESETS)[0];
    const opts = pre.PRESETS[presetName](msg.modelsRoot, { threads: msg.threads ?? 2, hotwords: msg.hotwords ?? pre.GAME_HOTWORDS });
    if (opts.modelDir && !existsSync(opts.modelDir)) throw new Error(`speech model not found: ${opts.modelDir}`);
    engine = eng.createAsrEngine(opts);
    engine.onPartial((p: { text: string }) => { if (p.text) post({ type: 'partial', text: p.text, t: Date.now() }); });
    engine.onFinal((f: { text: string; reason: string }) => {
      post({ type: 'final', text: f.text ?? '', reason: f.reason, t: Date.now(), audioMs: Math.round((audioSamples / 16000) * 1000), decodeMs: Date.now() - lastAudioAt });
      audioSamples = 0;
    });
    post({ type: 'ready', preset: presetName, loadMs: Date.now() - t0 });
  } catch (e) {
    post({ type: 'error', message: String((e as Error)?.message ?? e) });
  }
}

function pathToUrl(p: string) { return 'file:///' + p.replace(/\\/g, '/').replace(/^\/+/, ''); }

port.on('message', (e) => {
  const m = e.data;
  if (!m || typeof m !== 'object') return;
  try {
    switch (m.type) {
      case 'init': init(m); break;
      case 'pcm':
        if (!engine) return;
        if (!audioSamples) speechStart = Date.now();
        audioSamples += m.data.length;
        lastAudioAt = Date.now();
        engine.acceptPcm(m.data instanceof Float32Array ? m.data : new Float32Array(m.data));
        break;
      case 'end': if (engine) { lastAudioAt = Date.now(); engine.endUtterance(); } break;
      case 'reset': engine?.reset(); audioSamples = 0; break;
    }
  } catch (err) {
    post({ type: 'error', message: String((err as Error)?.message ?? err) });
  }
});
void speechStart;
