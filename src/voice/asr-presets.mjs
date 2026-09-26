// Model presets for asr-engine.mjs. Each preset maps a models root directory
// to createAsrEngine() options. Directory names match the sherpa-onnx release
// archives listed in docs/experiments/asr-models.manifest.json.
import { join } from 'node:path';

export const GAME_HOTWORDS = [
  'trampoline', 'catapult', 'tether', 'construct', 'drone', 'glove', 'magnet', 'carousel', 'baggage',
  'rocket', 'rockets', 'ramp', 'bridge', 'wrecking ball', 'conveyor belt', 'jetpack', 'harpoon',
  'grappling hook', 'anvil', 'seesaw', 'pendulum', 'turret', 'zipline', 'force field', 'portal',
  'piston', 'hinge', 'thruster', 'thrusters', 'balloon', 'balloons', 'cannon', 'spring', 'boxing glove',
  'ragdoll', 'chandelier', 'flamethrower', 'blimp', 'hovercraft', 'pinball flipper',
];

const vad = (m) => join(m, 'silero_vad.onnx');

export const PRESETS = {
  'zipformer-20M': (m, o = {}) => ({ kind: 'online-transducer', modelDir: join(m, 'sherpa-onnx-streaming-zipformer-en-20M-2023-02-17'), ...o }),
  'zipformer-0621': (m, o = {}) => ({ kind: 'online-transducer', modelDir: join(m, 'sherpa-onnx-streaming-zipformer-en-2023-06-21'), ...o }),
  'kroko-en': (m, o = {}) => ({ kind: 'online-transducer', modelDir: join(m, 'sherpa-onnx-streaming-zipformer-en-kroko-2025-08-06'), ...o }),
  'nemo-fc-80ms': (m, o = {}) => ({ kind: 'online-transducer', modelDir: join(m, 'sherpa-onnx-nemo-streaming-fast-conformer-transducer-en-80ms-int8'), ...o }),
  'nemo-fc-480ms': (m, o = {}) => ({ kind: 'online-transducer', modelDir: join(m, 'sherpa-onnx-nemo-streaming-fast-conformer-transducer-en-480ms-int8'), ...o }),
  'moonshine-tiny': (m, o = {}) => ({ kind: 'offline-moonshine', modelDir: join(m, 'sherpa-onnx-moonshine-tiny-en-int8'), vadModel: vad(m), ...o, hotwords: [] }),
  'moonshine-base': (m, o = {}) => ({ kind: 'offline-moonshine', modelDir: join(m, 'sherpa-onnx-moonshine-base-en-int8'), vadModel: vad(m), ...o, hotwords: [] }),
  'moonshine2-tiny': (m, o = {}) => ({ kind: 'offline-moonshine2', modelDir: join(m, 'sherpa-onnx-moonshine-tiny-en-quantized-2026-02-27'), vadModel: vad(m), ...o, hotwords: [] }),
  'moonshine2-base': (m, o = {}) => ({ kind: 'offline-moonshine2', modelDir: join(m, 'sherpa-onnx-moonshine-base-en-quantized-2026-02-27'), vadModel: vad(m), ...o, hotwords: [] }),
  'whisper-tiny.en': (m, o = {}) => ({ kind: 'offline-whisper', modelDir: join(m, 'sherpa-onnx-whisper-tiny.en'), vadModel: vad(m), ...o, hotwords: [] }),
  'whisper-base.en': (m, o = {}) => ({ kind: 'offline-whisper', modelDir: join(m, 'sherpa-onnx-whisper-base.en'), vadModel: vad(m), ...o, hotwords: [] }),
  'parakeet-tdt-v2': (m, o = {}) => ({ kind: 'offline-transducer', modelDir: join(m, 'sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8'), vadModel: vad(m), ...o }),
  'two-pass': (m, o = {}) => ({
    kind: 'two-pass', threads: o.threads,
    online: { modelDir: join(m, 'sherpa-onnx-nemo-streaming-fast-conformer-transducer-en-80ms-int8'), hotwords: o.hotwords ?? [] },
    offline: { kind: 'offline-moonshine2', modelDir: join(m, 'sherpa-onnx-moonshine-base-en-quantized-2026-02-27'), hotwords: [] },
    ...o,
  }),
};
