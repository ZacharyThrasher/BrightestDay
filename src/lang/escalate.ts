/**
 * System 2: a stronger generative model compiles an utterance the rule layer is unsure about into
 * the SAME bounded op vocabulary. It proposes; the interpreter compiles; the validator disposes.
 * The model never sees or produces executable code, and cannot bypass any limit.
 *
 * Providers:
 *   - AnthropicProvider      (player-supplied API key; default claude-haiku-4-5 for latency)
 *   - OpenAICompatProvider   (local Ollama / LM Studio / llama-server on the player's own PC)
 *   - ReplayProvider         (offline evaluation: recorded responses keyed by request id)
 *   - RecordProvider         (offline evaluation: writes requests, returns nothing)
 */
import { CONCEPTS, ADDONS } from './lexicon.js';

export interface S2Request { key: string; system: string; user: string }
export interface S2Provider { name: string; complete(req: S2Request, timeoutMs: number): Promise<string | null> }

export const S2_BEHAVIORS = `
strike{target,speed?,after?:"return"|"stay"|"dissolve",repeat?}  fly at target and hit it (punch, smash, ram)
grab{target,then:"hold"|"bring"|"lift"|"stay"}  seize target (bring = pull to player)
restrain{target}  trap target inside (cages, nets, bubbles)       crush{target}  heavy object dropped on target
orbit{around,radius?,speed?}  circle around a ref                  spin{rate?}  spin in place
follow{target}  stay near target                                   home{target,speed?}  chase target (missiles)
shoot{target,rate?,speed?}  fire hard-light bolts                   launch{speed?,dir:"up"|"arc"|"normal"|"forward"}  throws the player (and things) that touch it
cushion{}  soft catch for falling things                           reflect{}  bounces projectiles back to their shooter
field{mode:"attract"|"repel"|"wind"|"lift",radius?,strength?}       conveyor{speed?}  moving surface
thrust{accel?}  rockets pushing it forward                         explode{radius?,force?,delay?,onContact?}
lift{target,height?}  raise target (use target "self" for elevators) sweep{}  push forward clearing an area
mount{mode:"wear"|"ride",glide?,thrustLook?,thrustUp?,maxFall?,groundSpeedMul?,bounce?}  player wears/rides it (wings glide, jetpacks thrustUp, parachutes maxFall, vehicles groundSpeedMul)
tether{target:"none"}  rope from the player to this construct (grappling point)
hold{side?:"right"|"left"|"front"|"back"|"overhead"}  carried by the player`;

export function systemPrompt(): string {
  const concepts = CONCEPTS.map((c) => `${c.id} (${c.cls}${c.names.length > 1 ? ': ' + c.names.slice(1, 5).join(', ') : ''})`).join('; ');
  const addons = ADDONS.map((a) => a.id).join(', ');
  return `You compile a player's spoken request into construct operations for a hard-light superhero sandbox.
The player imagines objects made of solid light; you choose the closest composition of KNOWN building blocks.
You only PROPOSE. A deterministic validator clamps size/count/speed/energy and may reject. Never invent capabilities.

Respond with ONLY a JSON object: {"ops":[...], "summary":"<short>", "confidence":<0..1>}

op kinds:
{"op":"create","concept":"<concept id>","label":"<short name>","scale":<1=normal, 3=giant, 0.5=small>,"count":<int>,"arrangement":"single|line|ring|arc|stack|grid","placement":{"at":"<placement>","target":"<ref>"},"behaviors":[<behavior>],"addons":[{"id":"<addon>","where":"back|front|top|bottom|sides|around"}],"motion":"static|kinematic|dynamic","soft":<bool>}
{"op":"modify","target":"<construct id or focus>","changes":{"scale_mul":<n>,"stretch_mul":[x,y,z],"speed_mul":<n>,"count":<int>,"concept":"<new concept id>","add_behaviors":[<behavior>],"remove_behaviors":["<type>"],"addons":[...],"tilt":<deg>,"yaw":<deg>,"placement":{...}},"mods":["scale_up|scale_down|faster|slower|heavier|lighter|add_part|add_behavior|remove_behavior|duplicate|count_change|reorient|retarget|attach"]}
{"op":"act","target":"<construct id or focus>","action":"throw|release|dismiss","at":"<ref>"}
{"op":"control","action":"dismiss_all|undo|save|release_all","name":"<for save>"}
{"op":"abstain","reason":"<what to ask the player>"}

refs: "self" (the player), "aimed" (whatever is under the crosshair), "entity:<id>", "all_hostile", "nearest_enemy", "construct:<id>", "focus" (the construct the player is working with), "aim_point", "threat" (whoever is shooting).
placements: aim, front, ground_front (ramps ahead), self (around/on the player), under_self, behind_self, above_self, hand (in the player's hand), target, under_target, above_target, around_target, between (between player and threat/target), span (bridge from player to aim/target).
behaviors (JSON objects with "type" plus params; target fields take refs):${S2_BEHAVIORS}
concept ids: ${concepts}
addon ids: ${addons}

Rules:
- Compose: e.g. "boxing glove with rockets that punches the drone" = create boxing glove, addons rocket@back, behaviors [strike{target:"aimed"}].
- Unfamiliar objects: pick the concept with the same SHAPE and FUNCTION and say so in "label" (e.g. "vertical baggage carousel" -> carousel, tilt 90, reflect).
- Follow-ups ("bigger", "make them orbit me", "throw him into that wall") refer to the focus construct / held entity in the scene context. Use modify/act rather than creating new things.
- Goals ("so I can climb this", "get me across") must produce something that actually achieves them (launch/lift/tether/span/mount).
- Absurd sizes/counts: still request them (e.g. scale 1000000, count 10000); the validator clamps visibly.
- Abstain for: unsupported effects (invisibility, time, healing, teleport, mind control, real fire/water), self-contradictory requests, and genuinely unclear requests with no sensible default. Do not abstain when a sensible default exists.
- Keep confidence honest: below 0.5 when guessing.`;
}

export function userPrompt(utterance: string, context: any, s1: { summary: string; unknown: string[]; confidence: number }): string {
  return `Scene: ${JSON.stringify(context)}
Rule parser's attempt (may be wrong): ${s1.summary || '(nothing)'}${s1.unknown.length ? ` | unknown words: ${s1.unknown.join(', ')}` : ''} | its confidence ${s1.confidence.toFixed(2)}
Player said: "${utterance}"`;
}

export function extractJson(text: string): any | null {
  if (!text) return null;
  const t = text.replace(/```(?:json)?/g, '').trim();
  const start = t.indexOf('{');
  const end = t.lastIndexOf('}');
  if (start < 0 || end <= start) return null;
  try { return JSON.parse(t.slice(start, end + 1)); } catch { return null; }
}

// ----------------------------------------------------------------------------- providers

export class AnthropicProvider implements S2Provider {
  name: string;
  constructor(private apiKey: string, private model = 'claude-haiku-4-5-20251001', private url = 'https://api.anthropic.com/v1/messages') { this.name = `anthropic:${model}`; }
  async complete(req: S2Request, timeoutMs: number): Promise<string | null> {
    const ctl = new AbortController();
    const to = setTimeout(() => ctl.abort(), timeoutMs);
    try {
      const r = await fetch(this.url, {
        method: 'POST', signal: ctl.signal,
        headers: { 'content-type': 'application/json', 'x-api-key': this.apiKey, 'anthropic-version': '2023-06-01' },
        body: JSON.stringify({ model: this.model, max_tokens: 700, system: req.system, messages: [{ role: 'user', content: req.user }] }),
      });
      if (!r.ok) throw new Error(`HTTP ${r.status}: ${(await r.text()).slice(0, 200)}`);
      const j: any = await r.json();
      return (j.content ?? []).map((c: any) => c.text ?? '').join('');
    } finally { clearTimeout(to); }
  }
}

export class OpenAICompatProvider implements S2Provider {
  name: string;
  constructor(private baseUrl = 'http://127.0.0.1:11434/v1', private model = 'qwen2.5:7b-instruct', private apiKey = '') { this.name = `openai-compat:${model}`; }
  async complete(req: S2Request, timeoutMs: number): Promise<string | null> {
    const ctl = new AbortController();
    const to = setTimeout(() => ctl.abort(), timeoutMs);
    try {
      const r = await fetch(this.baseUrl.replace(/\/$/, '') + '/chat/completions', {
        method: 'POST', signal: ctl.signal,
        headers: { 'content-type': 'application/json', ...(this.apiKey ? { authorization: `Bearer ${this.apiKey}` } : {}) },
        body: JSON.stringify({ model: this.model, temperature: 0.1, max_tokens: 700, response_format: { type: 'json_object' }, messages: [{ role: 'system', content: req.system }, { role: 'user', content: req.user }] }),
      });
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      const j: any = await r.json();
      return j.choices?.[0]?.message?.content ?? null;
    } finally { clearTimeout(to); }
  }
}

/** Offline evaluation: responses recorded from a model, keyed by request key. */
export class ReplayProvider implements S2Provider {
  name = 'replay';
  constructor(private responses: Map<string, string>) {}
  async complete(req: S2Request): Promise<string | null> { return this.responses.get(req.key) ?? null; }
}

export class RecordProvider implements S2Provider {
  name = 'record';
  requests: S2Request[] = [];
  async complete(req: S2Request): Promise<string | null> { this.requests.push(req); return null; }
}
