/** Closed-class vocabulary for the rule-based System-1 parser. */

export type VerbClass =
  | 'create' | 'strike' | 'grab' | 'throw' | 'spin' | 'pull' | 'push' | 'lift' | 'crush' | 'trap' | 'block'
  | 'reflect' | 'carry' | 'shoot' | 'launch' | 'follow' | 'orbit' | 'sweep' | 'explode' | 'catch' | 'drop'
  | 'release' | 'dismiss' | 'undo' | 'save' | 'recall' | 'modify' | 'add' | 'remove' | 'duplicate' | 'stop'
  | 'protect' | 'cross' | 'climb' | 'descend' | 'rescue' | 'clear' | 'hold' | 'swing' | 'attract' | 'repel' | 'reel' | 'point' | 'ride' | 'handle'
  | 'locomote'; // walk / run / swim / hover / patrol / wander … (how a machine or creature gets about)

/** verb (lemma or inflection) → class. Multi-word verbs are matched first. */
export const VERBS: Record<string, VerbClass> = {
  // creation
  make: 'create', create: 'create', build: 'create', form: 'create', summon: 'create', conjure: 'create', construct: 'create',
  generate: 'create', spawn: 'create', give: 'create', gimme: 'create', manifest: 'create', craft: 'create', produce: 'create', materialize: 'create',
  materialise: 'create', imagine: 'create', draw: 'create', sculpt: 'create', shape: 'create', need: 'create', want: 'create', pop: 'create',
  slap: 'create', deploy: 'create', place: 'create', set: 'create', erect: 'create', raise: 'lift', 'put up': 'create', 'throw up': 'create', 'bring out': 'create',
  'how about': 'create', get: 'create', add: 'add', attach: 'add', put: 'create', stick: 'add', mount: 'add', bolt: 'add', strap: 'add', glue: 'add', tie: 'trap',
  fire: 'shoot', shoot: 'shoot', blast: 'shoot', zap: 'shoot', snipe: 'shoot', pew: 'shoot', spray: 'shoot', pelt: 'shoot',
  // strike
  punch: 'strike', hit: 'strike', smash: 'strike', whack: 'strike', bash: 'strike', slam: 'strike', pound: 'strike', strike: 'strike', swat: 'strike',
  smack: 'strike', bonk: 'strike', thump: 'strike', clobber: 'strike', wallop: 'strike', slug: 'strike', sock: 'strike', deck: 'strike', kick: 'strike',
  stab: 'strike', slash: 'strike', slice: 'strike', chop: 'strike', cut: 'strike', hammer: 'strike', ram: 'strike', headbutt: 'strike', uppercut: 'strike',
  jab: 'strike', poke: 'strike', attack: 'strike', destroy: 'strike', wreck: 'strike', beat: 'strike', 'take out': 'strike', have: 'create', 'knock out': 'strike', spank: 'strike',
  bop: 'strike', boop: 'strike', clock: 'strike', nail: 'strike', skewer: 'strike', impale: 'strike', dropkick: 'strike', karate: 'strike', demolish: 'strike',
  // grab
  grab: 'grab', seize: 'grab', snatch: 'grab', grip: 'grab', grasp: 'grab', 'pick up': 'grab', take: 'grab', nab: 'grab', clutch: 'grab', collect: 'grab', fetch: 'pull',
  pinch: 'grab', squeeze: 'crush', hold: 'hold', carry: 'carry', hug: 'grab', scoop: 'grab', 'scoop up': 'grab', pluck: 'grab', capture: 'trap', catch: 'catch',
  // throw
  throw: 'throw', toss: 'throw', fling: 'throw', hurl: 'throw', chuck: 'throw', yeet: 'throw', lob: 'throw', pitch: 'throw', send: 'throw', slingshot: 'throw',
  catapult: 'launch', launch: 'launch', bounce: 'launch', boost: 'launch', propel: 'launch', 'shoot up': 'launch',
  // spin / swing / orbit
  spin: 'spin', twirl: 'spin', whirl: 'spin', rotate: 'spin', 'spin around': 'spin', swing: 'swing', 'swing around': 'spin', whip: 'swing', helicopter: 'spin',
  orbit: 'orbit', circle: 'orbit', revolve: 'orbit', 'go around': 'orbit', surround: 'orbit', encircle: 'orbit', ring: 'orbit',
  // pull / push / lift / crush / trap
  pull: 'pull', yank: 'pull', drag: 'pull', bring: 'pull', tug: 'pull', haul: 'pull', reel: 'reel', lasso: 'pull', hook: 'pull', attract: 'attract', suck: 'attract', 'suck up': 'attract', vacuum: 'attract', magnetize: 'attract',
  push: 'push', shove: 'push', knock: 'push', 'knock back': 'push', nudge: 'push', bump: 'push', move: 'push', 'blow away': 'repel', blow: 'repel', repel: 'repel', deflect: 'reflect', reflect: 'reflect',
  'bounce back': 'reflect', 'send back': 'reflect', 'fling back': 'reflect', 'return': 'reflect', redirect: 'reflect', parry: 'reflect',
  lift: 'lift', hoist: 'lift', elevate: 'lift', levitate: 'lift', heave: 'lift', 'pick him up': 'grab',
  crush: 'crush', squash: 'crush', flatten: 'crush', crunch: 'crush', squish: 'crush', smoosh: 'crush', pancake: 'crush', stomp: 'crush', 'drop on': 'crush',
  trap: 'trap', cage: 'trap', restrain: 'trap', contain: 'trap', lock: 'trap', 'lock up': 'trap', imprison: 'trap', jail: 'trap', bind: 'trap', 'tie up': 'trap', pin: 'trap',
  wrap: 'trap', net: 'trap', freeze: 'trap', immobilize: 'trap', immobilise: 'trap', stop: 'stop', halt: 'stop', encase: 'trap', enclose: 'trap', bubble: 'trap', box: 'trap',
  // defence
  block: 'block', shield: 'protect', protect: 'protect', cover: 'protect', defend: 'protect', guard: 'protect', save: 'save', rescue: 'rescue', shelter: 'protect',
  // movement help
  ride: 'ride', drive: 'ride', fly: 'ride', glide: 'ride', surf: 'ride', cross: 'cross', climb: 'climb', 'get across': 'cross', 'get over': 'cross', 'get up': 'climb',
  'go up': 'climb', reach: 'climb', 'get down': 'descend', land: 'descend', descend: 'descend', jump: 'launch', leap: 'launch',
  // area
  sweep: 'sweep', clear: 'clear', bulldoze: 'sweep', plow: 'sweep', plough: 'sweep', 'get rid of': 'dismiss', explode: 'explode', 'blow up': 'explode', detonate: 'explode', nuke: 'explode', bomb: 'explode',
  // control
  drop: 'drop', release: 'release', 'let go': 'release', 'let go of': 'release', 'put down': 'drop', unhand: 'release', free: 'release', dismiss: 'dismiss', delete: 'dismiss',
  remove: 'remove', 'take off': 'remove', 'get rid': 'dismiss', vanish: 'dismiss', dispel: 'dismiss', unmake: 'dismiss', cancel: 'dismiss', 'never mind': 'undo', nevermind: 'undo',
  undo: 'undo', 'forget it': 'undo', 'scratch that': 'undo', revert: 'undo', 'call it': 'save', name: 'save', remember: 'save', store: 'save', recall: 'recall', use: 'recall',
  again: 'duplicate', duplicate: 'duplicate', copy: 'duplicate', clone: 'duplicate', double: 'duplicate', repeat: 'duplicate', 'another one': 'duplicate', 'one more': 'duplicate',
  turn: 'modify', change: 'modify', transform: 'modify', convert: 'modify', upgrade: 'modify', resize: 'modify', grow: 'modify', shrink: 'modify', stretch: 'modify',
  extend: 'modify', widen: 'modify', lengthen: 'modify', thicken: 'modify', flip: 'modify', tilt: 'modify', reverse: 'modify', invert: 'modify', speed: 'modify',
  enlarge: 'modify', expand: 'modify', inflate: 'modify', embiggen: 'modify', supersize: 'modify', upsize: 'modify', downsize: 'modify', lower: 'modify', aim: 'point', point: 'point', face: 'point', steer: 'point', redirect_to: 'point',
  flick: 'strike', tap: 'strike', thwack: 'strike', whap: 'strike', wham: 'strike', bat: 'strike', belt: 'strike', lob_: 'throw', ditch: 'dismiss', scrap: 'dismiss', lose: 'dismiss', bin: 'dismiss', trash: 'dismiss', wall: 'block', 'wall off': 'block', fence: 'trap', barricade: 'block', clap: 'crush', sandwich: 'crush', lever: 'lift', pry: 'lift', leverage: 'lift', crowbar: 'lift', jack: 'lift', roll: 'ride', deal: 'handle', handle: 'handle', 'take care of': 'handle', 'deal with': 'handle', 'do something': 'handle', help: 'handle', hurry: 'handle',
  follow: 'follow', chase: 'follow', track: 'follow', hunt: 'follow', pursue: 'follow', seek: 'follow', 'go after': 'follow', escort: 'follow',
  // machines & creatures
  kill: 'strike', bite: 'strike', chomp: 'strike', sting: 'strike', maul: 'strike', eat: 'strike', devour: 'strike', eliminate: 'strike', annihilate: 'strike', obliterate: 'strike', 'open fire': 'shoot',
  walk: 'locomote', run: 'locomote', swim: 'locomote', crawl: 'locomote', slither: 'locomote', hop: 'locomote', trot: 'locomote', gallop: 'locomote', march: 'locomote',
  scuttle: 'locomote', waddle: 'locomote', hover: 'locomote', patrol: 'locomote', wander: 'locomote', roam: 'locomote', prowl: 'locomote', stroll: 'locomote',
};

/** Order / machine vocabulary that is matched on raw tokens (not verbs of the ring) — kept here so typo repair knows it. */
export const MACHINE_WORDS = [
  'heel', 'sit', 'stay', 'come', 'fetch', 'patrol', 'wander', 'roam', 'flee', 'retreat', 'dismount', 'board', 'saddle', 'cockpit', 'seat', 'tentacle', 'tentacles',
  'treads', 'tracks', 'turret', 'turrets', 'rotor', 'rotors', 'propeller', 'propellers', 'flamethrower', 'missile', 'missiles', 'laser', 'lasers', 'cannon', 'cannons',
  'wheels', 'legs', 'wings', 'jaws', 'claws', 'pincers', 'fangs', 'teeth', 'tail', 'arms', 'guard', 'hunt', 'attack', 'follow', 'helicopter', 'tank', 'tanks', 'robot', 'robots',
  'spider', 'shark', 'dragon', 'machine', 'gun', 'guns', 'flying', 'walking', 'hovering', 'armed', 'army', 'swarm', 'squad', 'companion', 'buddy', 'pet', 'mech',
];

export const MULTI_VERBS = Object.keys(VERBS).filter((k) => k.includes(' ')).sort((a, b) => b.split(' ').length - a.split(' ').length);

/** Inflection → lemma for verbs (and a few nouns). */
export function lemma(w: string): string {
  if (VERBS[w]) return w;
  const tries: string[] = [];
  if (w.endsWith('ing')) tries.push(w.slice(0, -3), w.slice(0, -3) + 'e', w.slice(0, -4));
  if (w.endsWith('ied')) tries.push(w.slice(0, -3) + 'y');
  if (w.endsWith('ed')) tries.push(w.slice(0, -2), w.slice(0, -1), w.slice(0, -3));
  if (w.endsWith('es')) tries.push(w.slice(0, -2), w.slice(0, -1));
  if (w.endsWith('s')) tries.push(w.slice(0, -1));
  const irregular: Record<string, string> = { threw: 'throw', thrown: 'throw', flung: 'fling', hit: 'hit', made: 'make', built: 'build', caught: 'catch', brought: 'bring', held: 'hold', spun: 'spin', swung: 'swing', froze: 'freeze', frozen: 'freeze', took: 'take', gave: 'give', drew: 'draw', shot: 'shoot', blew: 'blow', flew: 'fly', drove: 'drive', rode: 'ride', stuck: 'stick', slung: 'sling', ran: 'run', got: 'get', put: 'put', set: 'set' };
  if (irregular[w]) return irregular[w];
  for (const t of tries) if (VERBS[t]) return t;
  return w;
}

export const SIZE_ADJ: Record<string, number> = {
  tiny: 0.3, teeny: 0.3, 'itty bitty': 0.25, miniature: 0.35, mini: 0.4, micro: 0.25, small: 0.6, little: 0.6, smaller: 0.65, smol: 0.5, compact: 0.7, petite: 0.6,
  medium: 1, normal: 1, regular: 1, standard: 1,
  big: 1.8, bigger: 1.5, large: 1.8, larger: 1.5, huge: 3, giant: 3, gigantic: 3.5, enormous: 3.5, massive: 3.5, colossal: 4, humongous: 4, immense: 4,
  mega: 3, jumbo: 2.5, oversized: 2.3, 'super big': 2.5, 'really big': 2.5, 'very big': 2.5, 'extra large': 2.5, xl: 2, xxl: 3, titanic: 4.5, monstrous: 4, mammoth: 4,
  tremendous: 3.5, hulking: 3, vast: 4, epic: 3, ginormous: 4.5, gargantuan: 5, godzilla: 6, monster: 3, king: 2.2, kingsize: 2.2, 'king size': 2.2, 'king-sized': 2.2,
  'house sized': 6, 'house-sized': 6, 'building sized': 10, 'building-sized': 10, 'car sized': 3, 'car-sized': 3, 'skyscraper sized': 30, 'mountain sized': 200, 'planet sized': 1e7,
  'planet-sized': 1e7, 'city sized': 5000, 'city-sized': 5000, 'moon sized': 1e6, 'moon-sized': 1e6, 'mountain-sized': 200, 'skyscraper-sized': 30, 'world sized': 1e7, infinite: 1e9, 'infinitely big': 1e9, 'infinitely large': 1e9,
};

export const DIM_ADJ: Record<string, [number, number, number]> = {
  long: [1, 1, 2], longer: [1, 1, 1.5], lengthy: [1, 1, 2], 'extra long': [1, 1, 3], 'super long': [1, 1, 3], 'really long': [1, 1, 3], 'mile long': [1, 1, 10], endless: [1, 1, 8],
  wide: [2, 1, 1], wider: [1.5, 1, 1], broad: [2, 1, 1], tall: [1, 2, 1], taller: [1, 1.5, 1], high: [1, 2, 1], higher: [1, 1.5, 1], towering: [1, 3, 1],
  thick: [1.6, 1.6, 1], thicker: [1.4, 1.4, 1], chunky: [1.5, 1.5, 1.5], fat: [1.6, 1.3, 1.6], thin: [0.5, 1, 0.5], thinner: [0.7, 1, 0.7], skinny: [0.5, 1, 0.5], slim: [0.6, 1, 0.6],
  flat: [1.3, 0.4, 1.3], flatter: [1, 0.7, 1], short: [1, 0.6, 0.6], shorter: [1, 0.7, 0.7], narrow: [0.5, 1, 1], narrower: [0.7, 1, 1], stubby: [1, 0.6, 0.6], deep: [1, 1, 1.6],
};

export const SPEED_ADJ: Record<string, number> = { 'ludicrously fast': 1000, ludicrous: 1000, 'insanely fast': 5, fast: 1.6, faster: 1.5, quick: 1.5, quicker: 1.4, rapid: 1.6, speedy: 1.6, 'super fast': 2.2, 'really fast': 2, 'lightning fast': 2.5, slow: 0.5, slower: 0.65, sluggish: 0.4, gentle: 0.6, gently: 0.6, softly: 0.6, hard: 1.4, harder: 1.4, 'full force': 1.8, 'as hard as you can': 2 };

export const MATERIAL_ADJ: Record<string, string> = {
  heavy: 'heavy', heavier: 'heavy', dense: 'heavy', weighted: 'heavy', solid: 'sturdy', sturdy: 'sturdy', strong: 'sturdy', stronger: 'sturdy', reinforced: 'sturdy', indestructible: 'sturdy', unbreakable: 'sturdy',
  light: 'light', lighter: 'light', lightweight: 'light', feather: 'light', floaty: 'light', hollow: 'light',
  bouncy: 'bouncy', bouncier: 'bouncy', springy: 'bouncy', rubbery: 'bouncy', elastic: 'bouncy', stretchy: 'bouncy', jiggly: 'bouncy',
  sticky: 'sticky', gluey: 'sticky', tacky: 'sticky', grippy: 'sticky',
  soft: 'soft', softer: 'soft', squishy: 'soft', padded: 'soft', cushioned: 'soft', fluffy: 'soft', pillowy: 'soft', comfy: 'soft', cushy: 'soft', spongy: 'soft',
  slippery: 'slippery', slick: 'slippery', icy: 'slippery', frictionless: 'slippery', greasy: 'slippery', smooth: 'slippery',
  spiky: 'spiky', spiked: 'spiky', thorny: 'spiky', pointy: 'spiky', sharp: 'spiky', serrated: 'spiky', barbed: 'spiky',
  spinning: 'spinning', rotating: 'spinning', whirling: 'spinning', twirling: 'spinning', spinny: 'spinning',
  exploding: 'explosive', explosive: 'explosive', explody: 'explosive', volatile: 'explosive',
  homing: 'homing', 'heat seeking': 'homing', 'heat-seeking': 'homing', guided: 'homing', 'self guided': 'homing', seeking: 'homing', smart: 'homing',
  floating: 'floating', hovering: 'floating', flying: 'floating', levitating: 'floating', airborne: 'floating',
  magnetic: 'magnetic', mirrored: 'reflective', reflective: 'reflective', shiny: 'reflective', mirror: 'reflective',
  rolling: 'rolling', vertical: 'vertical', upright: 'vertical', sideways: 'sideways', horizontal: 'flat', 'upside down': 'upsidedown', 'upside-down': 'upsidedown', backwards: 'reverse', reversed: 'reverse', inverted: 'upsidedown',
  permanent: 'permanent', temporary: 'temporary', quick_one: 'temporary',
};

export const INTENSIFIERS = new Set(['go', 'mate', 'fully', 'way', 'much', 'lot', 'really', 'super', 'even', 'so', 'too', 'bit', 'extra', 'very', 'totally', 'more', 'most', 'quite', 'pretty', 'real', 'hella', 'crazy', 'insanely', 'ridiculously', 'absurdly', 'stupidly', 'massively', 'seriously']);

export const IGNORED_ADJ = new Set([
  'green', 'red', 'blue', 'yellow', 'purple', 'pink', 'orange', 'white', 'black', 'gold', 'golden', 'silver', 'glowing', 'shiny', 'bright', 'neon', 'cool', 'awesome', 'nice', 'sick', 'epic', 'cute',
  'hard-light', 'hardlight', 'energy', 'glowy', 'sparkly', 'fancy', 'beautiful', 'pretty', 'ugly', 'scary', 'angry', 'friendly', 'good', 'great', 'awesome', 'lovely', 'dope', 'rad',
  'new', 'old', 'simple', 'basic', 'proper', 'real', 'actual', 'literal', 'regular', 'kind', 'sort', 'type', 'some', 'kinda', 'sorta', 'weird', 'funny', 'silly', 'crazy', 'wild', 'mad', 'cartoon', 'cartoonish',
  'massive-ass', 'big-ass', 'freaking', 'fricking', 'frickin', 'friggin', 'damn', 'dang', 'darn', 'bloody', 'heckin', 'absolute', 'absolutely', 'ultimate', 'super', 'uber', 'totally',
]);

export const COLOR_WORDS = new Set(['green', 'red', 'blue', 'yellow', 'purple', 'pink', 'orange', 'white', 'black', 'gold', 'golden', 'silver', 'cyan', 'teal', 'magenta', 'violet', 'rainbow', 'crimson', 'emerald', 'turquoise']);

export const NUMBERS: Record<string, number> = {
  one: 1, a: 1, an: 1, single: 1, two: 2, pair: 2, couple: 2, both: 2, three: 3, few: 3, four: 4, five: 5, six: 6, seven: 7, eight: 8, nine: 9, ten: 10, eleven: 11, twelve: 12,
  thirteen: 13, fifteen: 15, twenty: 20, thirty: 30, fifty: 50, hundred: 100, thousand: 1000, million: 1e6, billion: 1e9, dozen: 12, several: 4, bunch: 6, lot: 8, lots: 8, many: 8,
  tons: 20, gazillion: 1e9, bazillion: 1e9, zillion: 1e9, trillion: 1e12, infinite: 1e9, hundreds: 200, thousands: 2000, millions: 2e6, dozens: 36, some: 3, multiple: 3,
  twin: 2, triple: 3, double: 2, quadruple: 4, 'a bunch of': 6, 'a lot of': 8, 'a ton of': 20, 'a few': 3, 'a couple': 2, 'a couple of': 2, 'a pair of': 2, 'a dozen': 12, 'a swarm of': 10, 'a wall of': 6, 'a row of': 5, 'a line of': 5, 'a ring of': 6, 'a circle of': 6, 'a stack of': 4, 'a pile of': 6, 'an army of': 12, 'a fleet of': 6, 'a volley of': 6, 'a barrage of': 8, 'a hail of': 10, 'a storm of': 10, 'a shower of': 10, 'a rain of': 10,
};

export const ARRANGEMENT_WORDS: Record<string, string> = { row: 'line', line: 'line', wall: 'line', ring: 'ring', circle: 'ring', stack: 'stack', tower: 'stack', pile: 'scatter', grid: 'grid', swarm: 'scatter', army: 'grid', fleet: 'line', volley: 'line', barrage: 'scatter', hail: 'scatter', storm: 'scatter', shower: 'scatter', rain: 'scatter', arc: 'arc', fan: 'arc' };

export const FILLERS = new Set([
  'uh', 'uhh', 'um', 'umm', 'uhm', 'er', 'erm', 'hmm', 'hm', 'ah', 'oh', 'ok', 'okay', 'alright', 'please', 'pls', 'plz', 'just', 'quickly', 'quick', 'real', 'now', 'yeah', 'yes', 'yep', 'hey', 'ring',
  'basically', 'literally', 'actually', 'kinda', 'sorta', 'maybe', 'perhaps', 'totally', 'really', 'very', 'gonna', 'going', 'wanna', 'lets', "let's", 'can', 'could', 'would', 'will', 'you', 'u', 'i', 'we', 'should', 'shall', 'might', 'must', 'gotta',
  'something', 'thing', 'thingy', 'stuff', 'dude', 'bro', 'man', 'buddy', 'yo', 'okey', 'hold on', 'alrighty', 'aight', 'ahh', 'ooh', 'whoa', 'wow', 'ugh', 'well', 'right now', 'immediately', 'asap', 'already', 'come on', 'c\'mon',
]);

export const PREPOSITIONS = ['in front of', 'on top of', 'next to', 'up to', 'out of', 'away from', 'toward', 'towards', 'into', 'onto', 'at', 'to', 'on', 'under', 'underneath', 'beneath', 'below', 'over', 'above', 'around', 'behind', 'between', 'across', 'through', 'near', 'by', 'from', 'off', 'inside', 'with', 'against', 'beside', 'along', 'for', 'of', 'in', 'past', 'up', 'down', 'upon', 'round', 'onto'];

/** Words that refer to things already in the scene. */
export const ENTITY_NOUNS: Record<string, string> = {
  drone: 'drone', drones: 'drone', robot: 'robot', robots: 'robot', bot: 'robot', bots: 'robot', droid: 'robot', droids: 'robot', android: 'robot', androids: 'robot', mech: 'robot', mechs: 'robot', walker: 'robot', walkers: 'robot',
  enemy: 'hostile', enemies: 'hostile', bad: 'hostile', baddie: 'hostile', baddies: 'hostile', 'bad guys': 'hostile', target: 'hostile', targets: 'hostile', foe: 'hostile', foes: 'hostile', hostile: 'hostile', hostiles: 'hostile', villain: 'hostile', villains: 'hostile', monster: 'hostile', monsters: 'hostile',
  ufo: 'drone', ufos: 'drone', machine: 'drone', machines: 'drone', flyer: 'drone', flyers: 'drone', quadcopter: 'drone', 'flying thing': 'drone', buzzers: 'drone', buzzer: 'drone', mosquito: 'drone', mosquitoes: 'drone',
  turret: 'turret', turrets: 'turret', gun: 'turret', cannon: 'turret', shooter: 'turret', sentry: 'turret', tower_gun: 'turret', emplacement: 'turret', launcher: 'turret',
  civilian: 'civilian', civilians: 'civilian', guy: 'civilian', person: 'civilian', man: 'civilian', woman: 'civilian', lady: 'civilian', dude: 'civilian', kid: 'civilian', child: 'civilian', people: 'civilian',
  pedestrian: 'civilian', citizen: 'civilian', victim: 'civilian', faller: 'civilian', human: 'civilian', fella: 'civilian', fellow: 'civilian', girl: 'civilian', boy: 'civilian', worker: 'civilian', hostage: 'civilian',
  boulder: 'boulder', rock: 'boulder', stone: 'boulder', obstacle: 'boulder', rubble: 'boulder',
  crate: 'crate', crates: 'crate', box: 'crate', boxes: 'crate', barrel: 'crate',
  dummy: 'dummy', dummies: 'dummy', mannequin: 'dummy', 'training dummy': 'dummy', scarecrow: 'dummy',
};

/** World features the player may point at by name. */
export const PLACE_NOUNS = new Set(['wall', 'building', 'ground', 'floor', 'sky', 'air', 'ceiling', 'roof', 'rooftop', 'top', 'ledge', 'cliff', 'edge', 'gap', 'canyon', 'chasm', 'pit', 'hole', 'ravine', 'side', 'other side', 'far side', 'tower', 'door', 'doorway', 'gate', 'there', 'here', 'spot', 'area', 'place', 'path', 'way', 'street', 'road', 'river', 'city', 'distance', 'horizon', 'window', 'platform', 'goal']);

export const PRONOUNS_CONSTRUCT = new Set(['it', 'this', 'that', 'this one', 'that one', 'the last one', 'last one', 'the thing', 'my thing', 'the construct', 'the one', 'the same', 'same']);
export const PRONOUNS_PLURAL = new Set(['them', 'those', 'these', 'they', 'all of them', 'both', 'both of them', 'all', 'everything', 'everyone', 'everybody', 'each', 'each one', 'the lot']);
export const PRONOUNS_PERSON = new Set(['him', 'her', 'he', 'she', 'that guy', 'this guy', 'that thing', 'the guy', 'that dude', 'that one there', 'em', "'em"]);
export const SELF_WORDS = new Set(['me', 'myself', 'us', 'ourselves', 'i', 'my', 'mine', 'self']);
