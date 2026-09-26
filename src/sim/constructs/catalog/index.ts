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
import * as B from './beasts.js';
import * as M from './monsters.js';
import * as R from './rides.js';

export type { CatalogEntry } from './types.js';

/** Combat creatures, monsters and war vehicles. Listed before the core entries: their specific names ("wolf",
 * "griffin", "monster truck", "fighter jet", "titan") deliberately win over the core entries' catch-all synonyms. */
export const COMBAT_CATALOG: CatalogEntry[] = [
  // beasts
  B.wolf, B.raptor, B.bigcat, B.shark, B.bees, B.piranhas, B.bats, B.eagle, B.rhino, B.bull, B.elephant, B.bear, B.gorilla, B.trex,
  B.croc, B.kangaroo, B.scorpion, B.crab, B.spiderQueen, B.kraken, B.cobra, B.horse,
  // monsters
  M.golem, M.titan, M.cyclops, M.ogre, M.yeti, M.minotaur, M.werewolf, M.zombies, M.skeletons, M.mummy, M.ghost,
  M.iceDragon, M.stormDragon, M.hydra, M.kaiju, M.mechaKaiju, M.sandworm, M.griffin, M.phoenix,
  // rides & war machines
  R.monsterTruck, R.dozer, R.steamroller, R.train, R.drillTank, R.wreckingCrane, R.battleship, R.hovercraft, R.submarine,
  R.jetFighter, R.bomber, R.ufo, R.hoverbike, R.rocketRide, R.coaster, R.chariot,
];

export const CATALOG: CatalogEntry[] = [...COMBAT_CATALOG, tank, motorcycle, car, helicopter, airplane, mech, robot, dog, dragon, spider];

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
