import type { Part, Joint, Behavior, ConstructSpec, PlacementAt, Placement } from '../spec.js';

/** Same classes as the lexicon's ConceptClass (kept here so sim code does not import the language layer). */
export type CatalogClass =
  | 'barrier' | 'surface' | 'ramp' | 'launcher' | 'manipulator' | 'weapon' | 'heavy' | 'enclosure'
  | 'vehicle' | 'wearable' | 'projectile' | 'tool' | 'field' | 'rope' | 'creature' | 'container' | 'misc';

/**
 * One self-contained machine / creature. Registered into the lexicon as a concept (its names become words the
 * player can say) and into FORMS (its parts become a shape other concepts can reuse). Joints are re-attached by
 * validateSpec() whenever a spec's concept is this id and its grouped parts have no joints, so edits that keep the
 * parts ("make the tank bigger", "turn it into a tank") stay articulated.
 */
export interface CatalogEntry {
  id: string; // unique; also the form id and concept id
  names: string[]; // synonyms (the id is always a name). Earlier entries win name clashes.
  cls: CatalogClass;
  motion: ConstructSpec['motion']; // machines with 'drive' are always run kinematically
  at: PlacementAt; // 'self' + rideable = the player is put in it on creation
  orient?: Placement['orient'];
  scale?: number;
  density?: number;
  build(): { parts: Part[]; joints: Joint[] };
  behaviors: Behavior[];
  lifetime?: number;
  /** Observable functions this machine is meant to provide (claimed in the effect report, see eval/EVAL_SPEC_MACHINES.md). */
  functions: string[];
  /** What it does with a target but no verb ("a dog on that drone"). */
  verb?: string;
  /** Legacy lexicon concept ids this entry supersedes; their synonyms are inherited. */
  replaces?: string[];
}
