/**
 * Microphone capture in the renderer: getUserMedia -> AudioWorklet -> 16 kHz mono Float32 chunks (~100 ms).
 * Chunks are handed to a sink (the Electron preload forwards them to the speech utility process).
 */
const WORKLET = `
class Tap extends AudioWorkletProcessor {
  constructor() { super(); this.buf = new Float32Array(1600); this.n = 0; }
  process(inputs) {
    const ch = inputs[0] && inputs[0][0];
    if (ch) for (let i = 0; i < ch.length; i++) { this.buf[this.n++] = ch[i]; if (this.n === this.buf.length) { this.port.postMessage(this.buf.slice(0)); this.n = 0; } }
    return true;
  }
}
registerProcessor('hl-tap', Tap);`;

export interface MicCapture { start(): Promise<void>; stop(): void; level(): number; readonly active: boolean }

export function createMic(sink: (pcm: Float32Array) => void): MicCapture {
  let ctx: AudioContext | null = null;
  let stream: MediaStream | null = null;
  let node: AudioWorkletNode | null = null;
  let src: MediaStreamAudioSourceNode | null = null;
  let active = false;
  let rms = 0;
  return {
    get active() { return active; },
    level: () => rms,
    async start() {
      if (active) return;
      active = true;
      if (!ctx) {
        stream = await navigator.mediaDevices.getUserMedia({ audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true, autoGainControl: true } });
        ctx = new AudioContext({ sampleRate: 16000 }); // Chromium resamples the device to 16 kHz
        const url = URL.createObjectURL(new Blob([WORKLET], { type: 'application/javascript' }));
        await ctx.audioWorklet.addModule(url);
        src = ctx.createMediaStreamSource(stream);
        node = new AudioWorkletNode(ctx, 'hl-tap');
        node.port.onmessage = (e: MessageEvent<Float32Array>) => {
          if (!active) return;
          const d = e.data;
          let s = 0;
          for (let i = 0; i < d.length; i++) s += d[i] * d[i];
          rms = Math.sqrt(s / d.length);
          sink(d);
        };
        src.connect(node);
      }
      await ctx.resume();
    },
    stop() { active = false; rms = 0; },
  };
}
