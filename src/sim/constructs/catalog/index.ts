/**
 * The machine catalog: articulated, behaving constructs the ring knows by name.
 *
 * To add one: copy an entry (tank.ts is the fully commented template), give it a unique id + names, build parts
 * with groups + joints from kit.ts, list its behaviours (drive / brain / weapon / rideable, plus any classic
 * behaviour), and append it below. Earlier entries win name clashes. Nothing here is trusted: validateSpec()
 * clamps every entry exactly like a proposal from the parser or an LLM.
 */
import type { CatalogEntry } from './types.js';
import type { Joint, Part } from '../spec.js';
import { tank } from './tank.js';
import { car, motorcycle, helicopter, airplane } from './vehicles.js';
import { dog, dragon, spider, robot, mech } from './creatures.js';
import { WEAPONS } from './weapons.js';
import { TRAPS } from './traps.js';
import { SIEGE } from './siege.js';

export type { CatalogEntry } from './types.js';

/** Combat constructs (weapons / traps / siege) come after the vehicles and creatures, so those keep their names. */
export const COMBAT_CATALOG: CatalogEntry[] = [...WEAPONS, ...TRAPS, ...SIEGE];

export const CATALOG: CatalogEntry[] = [tank, motorcycle, car, helicopter, airplane, mech, robot, dog, dragon, spider, ...COMBAT_CATALOG];

export const CATALOG_BY_ID = new Map<string, CatalogEntry>(CATALOG.map((e) => [e.id, e]));

const builtCache = new Map<string, { parts: Part[]; joints: Joint[] }>();

/** Parts + joints of an entry (built once; callers get deep copies). */
export function buildEntry(id: string): { parts: Part[]; joints: Joint[] } | null {
  const e = CATALOG_BY_ID.get(id);
  if (!e) return null;
  let b = builtCache.get(id);
  if (!b) { b = e.build(); builtCache.set(id, b); }
  return JSON.parse(JSON.stringify(b));
}

/** Joints an entry defines, looked up for a (possibly edited) spec's concept id(s). */
export function catalogJoints(concept: string): Joint[] {
  const out: Joint[] = [];
  for (const id of concept.split(/[+,\s]/)) {
    const b = buildEntry(id);
    if (b) out.push(...b.joints);
  }
  return out;
}
