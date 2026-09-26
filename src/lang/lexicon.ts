/**
 * Concept lexicon = System-2 knowledge compiled ahead of time.
 *
 * Each concept is only (form + physical defaults + default placement + optional default behaviours).
 * Actions, targets, modifiers, counts and add-on parts are composed by the parser, so the lexicon
 * never needs an entry per phrase ("rocket glove that punches drones" is glove × rocket × strike × drone).
 *
 * Entries were authored by the implementing model as a distilled "recipe cache"; the frozen
 * held-out corpora were written independently and were not consulted.
 */
import type { PlacementAt, Placement, ConstructSpec } from '../sim/constructs/spec.js';

export type ConceptClass =
  | 'barrier' | 'surface' | 'ramp' | 'launcher' | 'manipulator' | 'weapon' | 'heavy' | 'enclosure'
  | 'vehicle' | 'wearable' | 'projectile' | 'tool' | 'field' | 'rope' | 'creature' | 'container' | 'misc';

export interface Concept {
  id: string;
  names: string[];
  form: string;
  cls: ConceptClass;
  motion: ConstructSpec['motion'];
  at: PlacementAt;
  orient?: Placement['orient'];
  scale?: number;
  density?: number;
  restitution?: number;
  friction?: number;
  soft?: boolean;
  /** behaviours attached on creation. 'T' in a target slot means "the clause target, else aim". */
  beh?: any[];
  /** what this thing does when used with a target but no verb ("an anvil on him") */
  verb?: string;
  lifetime?: number;
  /** machine/creature defaults: how it gets about when given autonomy or ridden, and whether it keeps you company */
  mach?: MachineDefaults;
}

export type LocoKind = 'wheels' | 'treads' | 'legs' | 'wings' | 'rotor' | 'propeller' | 'jets' | 'hover' | 'swim' | 'slither' | 'roll';
export interface MachineDefaults {
  loco: LocoKind;
  speed?: number;
  altitude?: number;
  /** default brain when made on its own ("a dog" keeps you company) */
  pet?: boolean;
  rideable?: boolean;
  /** natural weapon when told to attack and it has none */
  melee?: 'jaws' | 'arms' | 'claws' | 'ram' | 'sting';
  legs?: number;
}

const C = (id: string, names: string[], form: string, cls: ConceptClass, motion: Concept['motion'], at: PlacementAt, extra: Partial<Concept> = {}): Concept =>
  ({ id, names: [id, ...names], form, cls, motion, at, ...extra });

const T = { kind: 'T' } as any; // placeholder target resolved by the parser

export const CONCEPTS: Concept[] = [
  // --- barriers / defence
  C('wall', ['barrier', 'barricade', 'fence', 'blockade', 'partition', 'screen', 'bulwark', 'rampart'], 'wall', 'barrier', 'static', 'between', { verb: 'block' }),
  C('shield', ['buckler', 'aegis', 'deflector', 'riot shield', 'force field', 'forcefield', 'energy shield'], 'shield', 'barrier', 'kinematic', 'front', { beh: [{ type: 'hold', side: 'front' }], verb: 'block' }),
  C('panel', ['sheet', 'plate', 'slab of light', 'pane', 'board of light', 'mirror'], 'panel', 'barrier', 'static', 'between', { verb: 'block' }),
  C('door', ['gate', 'hatch'], 'wall', 'barrier', 'static', 'between', { scale: 0.7 }),
  C('dome', ['igloo', 'shelter', 'canopy dome', 'half dome', 'force dome'], 'dome', 'enclosure', 'static', 'self', { verb: 'protect' }),
  C('bubble', ['orb', 'sphere shield', 'force bubble', 'ball shield', 'globe', 'snow globe'], 'bubble', 'enclosure', 'static', 'self', { verb: 'protect' }),
  C('armor', ['armour', 'suit of armor', 'suit of armour', 'shell', 'turtle shell', 'carapace', 'exoskeleton', 'power armor', 'power armour', 'mech suit', 'battle suit', 'body armor', 'body armour', 'force suit', 'bubble suit', 'riot gear', 'plate armor', 'knight armor'], 'sphere_shell', 'barrier', 'kinematic', 'self', { scale: 0.62, beh: [{ type: 'follow', target: { kind: 'self' }, offset: [0, 0, 0] }], verb: 'protect' }),
  C('umbrella', ['parasol'], 'umbrella', 'wearable', 'kinematic', 'above_self', { beh: [{ type: 'mount', mode: 'wear', maxFall: 6 }] }),
  // --- surfaces & traversal
  C('platform', ['floor', 'ledge', 'step', 'stage', 'pad', 'deck', 'raft', 'tile', 'disc platform', 'hover platform', 'landing pad'], 'platform', 'surface', 'static', 'under_self'),
  C('slab', ['plank', 'beam', 'girder', 'board', 'walkway', 'catwalk', 'bench', 'table'], 'slab', 'surface', 'static', 'aim'),
  C('bridge', ['overpass', 'span', 'causeway', 'drawbridge', 'gangplank', 'rope bridge', 'footbridge', 'path', 'road', 'highway', 'runway', 'rail', 'rails', 'track', 'grind rail'], 'bridge', 'surface', 'static', 'span'),
  C('ramp', ['jump', 'kicker', 'incline', 'slope', 'wedge', 'half pipe', 'halfpipe', 'ski jump', 'launch ramp', 'quarter pipe'], 'ramp', 'ramp', 'static', 'ground_front'),
  C('stairs', ['staircase', 'steps', 'stairway', 'escalator', 'stair'], 'stairs', 'ramp', 'static', 'ground_front'),
  C('ladder', ['rungs'], 'ladder', 'ramp', 'static', 'aim'),
  C('slide', ['chute', 'water slide', 'slip and slide'], 'slide', 'ramp', 'static', 'ground_front', { friction: 0 }),
  C('pillar', ['column', 'pole', 'post', 'pylon', 'totem', 'obelisk', 'stilt', 'stilts'], 'pillar', 'surface', 'static', 'aim'),
  C('tower', ['skyscraper', 'lighthouse', 'spire', 'watchtower', 'scaffold', 'scaffolding', 'building'], 'tower', 'surface', 'static', 'aim'),
  C('elevator', ['lift platform', 'elevator platform', 'hoist', 'dumbwaiter'], 'platform', 'surface', 'kinematic', 'under_self', { beh: [{ type: 'lift', target: { kind: 'self' }, height: 20 }] }),
  C('anchor', ['grapple point', 'anchor point', 'hook point', 'handle', 'ring anchor', 'swing point', 'monkey bar', 'monkey bars'], 'anchor', 'rope', 'static', 'aim'),
  C('rope', ['cable', 'line', 'tether', 'cord', 'string', 'vine', 'zipline', 'zip line', 'grappling line', 'bungee', 'bungee cord', 'rope swing', 'tightrope'], 'rope', 'rope', 'static', 'aim', { beh: [{ type: 'tether', target: { kind: 'none' }, pull: 16, toPlayer: true }] }),
  C('grappling hook', ['grapple', 'grappling', 'hookshot', 'grappling gun'], 'hook', 'rope', 'static', 'aim', { beh: [{ type: 'tether', target: { kind: 'none' }, pull: 16, toPlayer: true }] }),
  C('hook', ['fishhook', 'fish hook', 'gaff', 'crook', 'shepherd crook'], 'hook', 'manipulator', 'kinematic', 'hand', { verb: 'pull' }),
  C('conveyor', ['conveyor belt', 'treadmill', 'moving walkway', 'assembly line', 'belt'], 'conveyor', 'surface', 'static', 'ground_front', { beh: [{ type: 'conveyor', speed: 14, axis: [0, 0, 1] }] }),
  C('carousel', ['baggage carousel', 'luggage carousel', 'merry go round', 'merry-go-round', 'roundabout', 'turntable', 'lazy susan', 'loop'], 'carousel', 'surface', 'kinematic', 'aim', { beh: [{ type: 'conveyor', speed: 12, axis: [0, 0, 1] }] }),
  // --- launchers
  C('trampoline', ['trampolene', 'bounce pad', 'bouncy castle', 'bouncy house', 'mattress', 'bed'], 'trampoline', 'launcher', 'static', 'aim', { soft: true, restitution: 0.9, beh: [{ type: 'launch', speed: 16, dir: 'normal' }, { type: 'cushion' }] }),
  C('spring', ['coil', 'spring board', 'springboard', 'diving board', 'jack in the box', 'pogo stick', 'pogo'], 'spring', 'launcher', 'static', 'ground_front', { beh: [{ type: 'launch', speed: 30, dir: 'up' }] }),
  C('launch pad', ['launchpad', 'jump pad', 'boost pad', 'booster pad', 'geyser', 'jump plate', 'bounce plate'], 'launchpad', 'launcher', 'static', 'ground_front', { beh: [{ type: 'launch', speed: 34, dir: 'up' }] }),
  C('catapult', ['trebuchet', 'slingshot', 'sling', 'ballista', 'onager', 'mangonel', 'seesaw', 'see saw', 'teeter totter', 'lever'], 'catapult', 'launcher', 'static', 'ground_front', { beh: [{ type: 'launch', speed: 38, dir: 'arc' }] }),
  C('cannon', ['artillery', 'mortar', 'howitzer', 'bazooka', 'rocket launcher', 'launcher', 'potato gun', 't-shirt cannon'], 'cannon', 'weapon', 'static', 'front', { beh: [{ type: 'shoot', target: T, rate: 1.5, speed: 50, size: 0.6 }], verb: 'shoot' }),
  // --- manipulators
  C('hand', ['palm', 'mitt', 'fingers', 'giant hand', 'grabber', 'grabbing hand', 'arm'], 'hand', 'manipulator', 'kinematic', 'hand', { verb: 'grab', scale: 1.8 }),
  C('claw', ['talon', 'pincer', 'pincers', 'crane claw', 'claw machine', 'tongs', 'pliers', 'forceps', 'tweezers', 'grip', 'clamp', 'vice', 'vise', 'crab claw'], 'claw', 'manipulator', 'kinematic', 'hand', { verb: 'grab', scale: 1.8 }),
  C('lasso', ['lariat', 'whip', 'tentacle', 'tongue', 'noose', 'rope loop'], 'whip', 'manipulator', 'kinematic', 'hand', { verb: 'pull' }),
  C('magnet', ['electromagnet', 'tractor beam', 'attractor', 'gravity well', 'black hole', 'singularity'], 'magnet', 'field', 'kinematic', 'front', { beh: [{ type: 'field', mode: 'attract', radius: 14, strength: 40 }], verb: 'attract' }),
  C('vacuum', ['vacuum cleaner', 'hoover', 'suction', 'sucker', 'dyson'], 'funnel', 'field', 'kinematic', 'front', { beh: [{ type: 'field', mode: 'attract', radius: 16, strength: 35 }], verb: 'attract' }),
  C('fan', ['blower', 'leaf blower', 'turbine', 'jet engine', 'hair dryer', 'hairdryer', 'windmill', 'bellows'], 'fan', 'field', 'static', 'front', { beh: [{ type: 'field', mode: 'wind', radius: 18, strength: 45 }, { type: 'spin', rate: 18, axis: [0, 0, 1] }], verb: 'push' }),
  C('tornado', ['twister', 'whirlwind', 'cyclone', 'hurricane', 'dust devil', 'waterspout'], 'tornado', 'field', 'kinematic', 'aim', { beh: [{ type: 'field', mode: 'lift', radius: 10, strength: 40 }, { type: 'spin', rate: 6, axis: [0, 1, 0] }], lifetime: 12 }),
  C('vortex', ['whirlpool', 'portal', 'wormhole', 'spiral'], 'vortex', 'field', 'kinematic', 'aim', { beh: [{ type: 'field', mode: 'attract', radius: 12, strength: 35 }, { type: 'spin', rate: 5, axis: [0, 0, 1] }], lifetime: 12 }),
  C('pusher', ['piston', 'ram', 'battering ram', 'bulldozer', 'plow', 'snowplow', 'snow plow', 'shovel', 'dozer', 'push block'], 'block', 'tool', 'kinematic', 'front', { scale: 2.2, verb: 'push' }),
  C('broom', ['sweeper', 'mop', 'rake', 'squeegee'], 'slab', 'tool', 'kinematic', 'front', { scale: 0.6, beh: [{ type: 'sweep', width: 16, speed: 14 }], verb: 'sweep' }),
  // --- weapons / strikers
  C('fist', ['punch', 'knuckles', 'brass knuckles'], 'fist', 'weapon', 'kinematic', 'hand', { verb: 'strike', scale: 1.8 }),
  C('boxing glove', ['glove', 'gauntlet', 'mitten', 'oven mitt'], 'glove', 'weapon', 'kinematic', 'hand', { verb: 'strike', scale: 1.4 }),
  C('foot', ['boot', 'kick', 'shoe', 'sneaker', 'leg'], 'foot', 'weapon', 'kinematic', 'hand', { verb: 'strike', scale: 1.8 }),
  C('hammer', ['sledgehammer', 'sledge', 'warhammer', 'war hammer', 'maul', 'gavel'], 'hammer', 'weapon', 'kinematic', 'hand', { verb: 'strike', scale: 1.5 }),
  C('mallet', ['cartoon mallet', 'croquet mallet', 'tenderizer'], 'mallet', 'weapon', 'kinematic', 'hand', { verb: 'strike', scale: 1.6 }),
  C('sword', ['blade', 'katana', 'saber', 'sabre', 'machete', 'knife', 'dagger', 'longsword', 'claymore', 'cutlass', 'scimitar', 'lightsaber'], 'sword', 'weapon', 'kinematic', 'hand', { verb: 'strike', scale: 1.3 }),
  C('axe', ['hatchet', 'battle axe', 'battleaxe', 'cleaver', 'tomahawk', 'pickaxe', 'pick'], 'axe', 'weapon', 'kinematic', 'hand', { verb: 'strike', scale: 1.4 }),
  C('bat', ['baseball bat', 'cricket bat', 'paddle', 'racket', 'racquet', 'tennis racket', 'golf club', 'hockey stick', 'stick', 'rolling pin', 'frying pan', 'pan', 'swatter', 'fly swatter', 'flyswatter', 'newspaper', 'spatula'], 'bat', 'weapon', 'kinematic', 'hand', { verb: 'strike', scale: 1.6 }),
  C('club', ['cudgel', 'baton', 'truncheon', 'mace', 'morning star', 'nightstick', 'flail'], 'club', 'weapon', 'kinematic', 'hand', { verb: 'strike', scale: 1.5 }),
  C('spear', ['lance', 'javelin', 'pike', 'trident', 'harpoon', 'halberd', 'polearm', 'jousting lance', 'pitchfork'], 'spear', 'weapon', 'kinematic', 'hand', { verb: 'strike', scale: 1.5 }),
  C('arrow', ['dart', 'bolt', 'quarrel', 'needle'], 'arrow', 'projectile', 'kinematic', 'hand', { verb: 'strike' }),
  C('bow', ['crossbow', 'longbow'], 'bow', 'weapon', 'kinematic', 'hand', { beh: [{ type: 'hold', side: 'left' }, { type: 'shoot', target: T, rate: 1.2, speed: 60, size: 0.25 }], verb: 'shoot' }),
  C('blaster', ['gun', 'pistol', 'rifle', 'ray gun', 'laser', 'laser gun', 'blaster rifle', 'minigun', 'machine gun', 'shotgun', 'water gun', 'squirt gun', 'nerf gun', 'nail gun', 'staple gun', 'flamethrower', 'sniper'], 'blaster', 'weapon', 'kinematic', 'hand', { beh: [{ type: 'hold', side: 'right' }, { type: 'shoot', target: T, rate: 3, speed: 55, size: 0.25 }], verb: 'shoot' }),
  C('turret', ['sentry', 'sentry gun', 'gun turret', 'auto turret'], 'turret', 'weapon', 'static', 'aim', { beh: [{ type: 'shoot', target: { kind: 'nearest_enemy' }, rate: 2, speed: 50, size: 0.3 }], verb: 'shoot' }),
  C('saw', ['buzzsaw', 'buzz saw', 'saw blade', 'circular saw', 'sawblade', 'chainsaw', 'shuriken', 'ninja star', 'throwing star', 'frisbee', 'disc', 'discus', 'chakram', 'boomerang', 'pizza cutter'], 'saw', 'weapon', 'kinematic', 'hand', { beh: [{ type: 'spin', rate: 25, axis: [0, 0, 1] }], verb: 'strike' }),
  C('drill', ['auger', 'jackhammer', 'screw', 'corkscrew'], 'drill', 'weapon', 'kinematic', 'hand', { beh: [{ type: 'spin', rate: 20, axis: [0, 0, 1] }], verb: 'strike' }),
  C('gear', ['cog', 'cogwheel', 'sprocket'], 'gear', 'weapon', 'kinematic', 'aim', { beh: [{ type: 'spin', rate: 8, axis: [0, 0, 1] }] }),
  C('wheel', ['tire', 'tyre', 'hamster wheel', 'ferris wheel', 'hoop wheel', 'roller'], 'wheel', 'misc', 'dynamic', 'front', { density: 600 }),
  C('rocket', ['missile', 'torpedo', 'firework', 'bottle rocket', 'rpg', 'warhead'], 'rocket', 'projectile', 'kinematic', 'hand', { beh: [{ type: 'home', target: T, speed: 38 }, { type: 'explode', radius: 6, force: 32, delay: 6, onContact: true }], verb: 'strike' }),
  C('bomb', ['grenade', 'explosive', 'dynamite', 'tnt', 'mine', 'charge', 'firecracker', 'bomb ball'], 'ball', 'projectile', 'dynamic', 'hand', { beh: [{ type: 'explode', radius: 7, force: 35, delay: 2, onContact: false }], verb: 'throw' }),
  C('ball', ['sphere', 'orb ball', 'globe ball', 'marble', 'bowling ball', 'cannonball', 'cannon ball', 'basketball', 'soccer ball', 'football', 'beach ball', 'dodgeball', 'snowball', 'pinball', 'medicine ball', 'beach ball'], 'ball', 'projectile', 'dynamic', 'hand', { density: 500, restitution: 0.5, verb: 'throw' }),
  // --- heavy things
  C('anvil', ['safe', 'piano', 'fridge', 'refrigerator', 'vending machine', 'dumpster', 'car crusher'], 'anvil', 'heavy', 'dynamic', 'above_target', { density: 5000, verb: 'crush' }),
  C('weight', ['ton', 'ten ton weight', 'dumbbell', 'kettlebell', 'barbell', 'counterweight', 'brick', 'cinder block', 'block'], 'weight', 'heavy', 'dynamic', 'above_target', { density: 4500, verb: 'crush' }),
  C('boulder', ['rock', 'stone', 'meteor', 'asteroid', 'meteorite', 'giant rock', 'moon'], 'boulder', 'heavy', 'dynamic', 'above_target', { density: 2500, verb: 'crush' }),
  C('wrecking ball', ['wrecking-ball', 'pendulum ball', 'demolition ball'], 'wrecking_ball', 'heavy', 'kinematic', 'hand', { density: 4000, verb: 'strike', scale: 1.2 }),
  C('pendulum', ['swinging axe', 'swinging blade'], 'pendulum', 'heavy', 'kinematic', 'hand', { density: 3000, verb: 'strike' }),
  C('cube', ['box', 'crate', 'container', 'shipping container', 'dice', 'die'], 'cube', 'misc', 'dynamic', 'aim', { density: 400 }),
  C('pillow', ['cushion', 'airbag', 'air bag', 'bean bag', 'beanbag', 'crash pad', 'crash mat', 'mat', 'marshmallow', 'foam pit', 'ball pit', 'hay bale', 'haystack', 'couch', 'sofa', 'blanket'], 'pillow', 'surface', 'static', 'under_target', { soft: true, beh: [{ type: 'cushion' }] }),
  // --- enclosures / restraints
  C('cage', ['jail', 'prison', 'cell', 'birdcage', 'bird cage', 'pen', 'coop', 'kennel', 'trap', 'playpen', 'crib', 'lockbox'], 'cage', 'enclosure', 'kinematic', 'around_target', { beh: [{ type: 'restrain', target: T }], verb: 'trap' }),
  C('net', ['netting', 'web', 'spiderweb', 'spider web', 'mesh', 'hammock', 'safety net', 'fishing net', 'butterfly net', 'catch net', 'sail'], 'net', 'enclosure', 'static', 'under_target', { soft: true, beh: [{ type: 'cushion' }], verb: 'catch' }),
  C('box trap', ['tupperware', 'jar', 'bottle', 'glass box', 'fishbowl', 'fish bowl', 'aquarium', 'tank of light'], 'box_open', 'enclosure', 'kinematic', 'around_target', { beh: [{ type: 'restrain', target: T }], verb: 'trap' }),
  C('bucket', ['cup', 'bowl', 'basket', 'pail', 'scoop', 'catcher mitt', 'catchers mitt', 'baseball mitt', 'bin', 'trash can', 'barrel', 'pot', 'wok', 'ladle'], 'bucket', 'container', 'kinematic', 'under_target', { soft: true, beh: [{ type: 'cushion' }], verb: 'catch' }),
  C('ring', ['hoop', 'halo', 'lifesaver', 'life preserver', 'donut', 'doughnut', 'torus', 'bracelet', 'collar', 'handcuffs', 'cuffs', 'shackles'], 'ring', 'enclosure', 'kinematic', 'around_target', { verb: 'trap' }),
  C('tube', ['pipe', 'tunnel', 'straw', 'culvert', 'sewer pipe', 'water pipe', 'cylinder', 'barrel of light', 'toilet paper roll'], 'tube', 'surface', 'static', 'ground_front'),
  C('funnel', ['cone', 'megaphone', 'traffic cone'], 'funnel', 'container', 'static', 'aim'),
  // --- vehicles & wearables
  C('car', ['truck', 'van', 'bus', 'jeep', 'go kart', 'go-kart', 'kart', 'buggy', 'dune buggy', 'race car', 'racecar', 'motorcycle', 'motorbike', 'bike', 'bicycle', 'scooter', 'segway', 'wagon', 'cart', 'shopping cart', 'wheelchair', 'monster truck', 'quad bike', 'atv', 'tricycle', 'unicycle', 'train', 'locomotive', 'tram', 'trolley', 'minecart', 'mine cart', 'sled', 'sleigh', 'bobsled', 'toboggan', 'chariot', 'rickshaw', 'lawnmower', 'lawn mower', 'forklift', 'tractor', 'bulldozer car', 'snowmobile'], 'car', 'vehicle', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'ride', groundSpeedMul: 2.4, frictionMul: 0.4 }], mach: { loco: 'wheels', speed: 18, rideable: true, melee: 'ram' } }),
  C('hamster ball', ['zorb', 'zorb ball', 'rolling ball', 'giant hamster ball'], 'bubble', 'vehicle', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'ride', groundSpeedMul: 2.2, frictionMul: 0.3 }], mach: { loco: 'roll', speed: 14, rideable: true, melee: 'ram' } }),
  C('skateboard', ['longboard', 'roller skates', 'rollerblades', 'skates', 'ice skates', 'skis', 'ski', 'snowboard'], 'skateboard', 'vehicle', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'ride', groundSpeedMul: 2.0, frictionMul: 0.15 }], mach: { loco: 'wheels', speed: 14, rideable: true, melee: 'ram' } }),
  C('hoverboard', ['surfboard', 'flying surfboard', 'surf board', 'hover board', 'magic carpet', 'carpet', 'flying carpet', 'broomstick', 'flying broom', 'hover disc', 'flying saucer', 'ufo', 'hover bike', 'hoverbike', 'jet ski', 'jetski', 'cloud', 'nimbus'], 'surfboard', 'vehicle', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'ride', glide: 1.6, thrustLook: 14, maxFall: 10 }], mach: { loco: 'hover', speed: 16, altitude: 1.5, rideable: true, melee: 'ram' } }),
  C('wings', ['wing', 'wingsuit', 'wing suit', 'angel wings', 'bat wings', 'butterfly wings', 'feathers', 'cape', 'hang glider', 'kite'], 'wings', 'wearable', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'wear', glide: 1.3 }] }),
  C('glider', ['paraglider', 'sailplane', 'glider wing'], 'glider', 'wearable', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'wear', glide: 1.6, maxFall: 12 }] }),
  C('parachute', ['chute', 'parasail', 'balloon', 'hot air balloon', 'balloons', 'bunch of balloons', 'blimp', 'zeppelin', 'feather', 'dandelion'], 'parachute', 'wearable', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'wear', maxFall: 4 }] }),
  C('jetpack', ['jet pack', 'rocket pack', 'rocket boots', 'jet boots', 'thrusters', 'rocket shoes', 'rocketpack', 'backpack rockets', 'booster pack'], 'jetpack', 'wearable', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'wear', thrustUp: 34, glide: 0.3 }] }),
  C('pogo boots', ['spring shoes', 'spring boots', 'moon boots', 'bouncy shoes', 'kangaroo boots', 'spring legs'], 'spring', 'wearable', 'kinematic', 'self', { scale: 0.35, beh: [{ type: 'mount', mode: 'wear', bounce: 16 }] }),
  C('plane', ['airplane', 'aeroplane', 'jet', 'fighter jet', 'biplane', 'airliner', 'paper airplane', 'paper plane', 'spaceship', 'space ship', 'starship', 'shuttle', 'space shuttle', 'bird plane'], 'plane', 'vehicle', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'ride', glide: 1.8, thrustLook: 18 }], mach: { loco: 'propeller', speed: 30, altitude: 15, rideable: true, melee: 'ram' } }),
  C('helicopter', ['chopper', 'drone ship', 'quadcopter', 'gyrocopter', 'propeller hat', 'propeller beanie', 'heli', 'helo', 'gunship'], 'helicopter', 'vehicle', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'ride', thrustUp: 30, maxFall: 8 }], mach: { loco: 'rotor', speed: 14, altitude: 10, rideable: true, melee: 'ram' } }),
  C('boat', ['canoe', 'kayak', 'rowboat', 'sailboat', 'ship', 'yacht', 'submarine', 'surf raft', 'dinghy', 'ark', 'gondola'], 'boat', 'vehicle', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'ride', groundSpeedMul: 1.8, frictionMul: 0.3, glide: 0.5 }], mach: { loco: 'hover', speed: 12, altitude: 0.8, rideable: true, melee: 'ram' } }),
  // --- creatures (approximations; visibly flagged as approximated by the parser)
  C('dragon', ['wyvern', 'phoenix', 'griffin', 'gryphon', 'pegasus', 'pterodactyl', 'eagle ride'], 'dragon', 'creature', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'ride', glide: 2, thrustLook: 16, thrustUp: 20 }], mach: { loco: 'wings', speed: 18, altitude: 12, rideable: true, melee: 'jaws' } }),
  C('bird', ['eagle', 'hawk', 'falcon', 'crow', 'raven', 'owl', 'pigeon', 'bat creature', 'bee', 'wasp', 'hornet', 'mosquito', 'fly', 'butterfly', 'dragonfly', 'parrot', 'seagull'], 'bird', 'creature', 'kinematic', 'hand', { verb: 'strike', mach: { loco: 'wings', speed: 12, altitude: 6, pet: true, melee: 'sting' } }),
  C('horse', ['pony', 'unicorn', 'camel', 'elephant', 'rhino', 'rhinoceros', 'bull', 'ox', 'dinosaur', 't-rex', 'trex', 'triceratops', 'deer', 'moose', 'cow', 'goat', 'donkey', 'zebra', 'giraffe'], 'horse', 'creature', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'ride', groundSpeedMul: 2.2, frictionMul: 0.5 }], mach: { loco: 'legs', speed: 12, rideable: true, melee: 'ram' } }),
  C('snake', ['serpent', 'python', 'anaconda', 'boa', 'worm', 'eel', 'octopus', 'squid', 'kraken', 'centipede', 'caterpillar'], 'snake', 'creature', 'kinematic', 'around_target', { beh: [{ type: 'restrain', target: T }], verb: 'trap', mach: { loco: 'slither', speed: 6, melee: 'jaws' } }),
  C('dog', ['hound', 'wolf', 'puppy', 'doggo', 'pupper', 'cat', 'kitten', 'tiger', 'lion', 'bear', 'gorilla', 'monkey', 'crocodile', 'alligator', 'fox', 'panther', 'raptor', 'mouse', 'rabbit', 'bunny', 'cheetah', 'leopard', 'hyena', 'lizard', 'frog', 'turtle'], 'quadruped', 'creature', 'kinematic', 'hand', { verb: 'grab', mach: { loco: 'legs', speed: 9, pet: true, melee: 'jaws' } }),
  // ---- placeholder machine bodies (language layer); a machine catalog may supersede these ----
  C('tank', ['panzer', 'battle tank', 'army tank', 'war tank', 'armored car', 'armoured car', 'tankette'], 'tank', 'vehicle', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'ride', groundSpeedMul: 1.6, frictionMul: 0.6 }, { type: 'weapon', kind: 'cannon', group: 'barrel', rate: 0.8, speed: 50, damage: 35, range: 70 }], mach: { loco: 'treads', speed: 9, rideable: true, melee: 'ram' } }),
  C('robot', ['bot', 'android', 'droid', 'robot buddy', 'robo', 'mech', 'mecha', 'giant robot', 'battle robot', 'golem', 'cyborg', 'automaton', 'terminator', 'humanoid', 'knight', 'soldier', 'ninja', 'zombie', 'troll', 'ogre', 'yeti'], 'humanoid', 'creature', 'kinematic', 'ground_front', { mach: { loco: 'legs', speed: 6, pet: true, rideable: true, melee: 'arms', legs: 2 } }),
  C('spider', ['tarantula', 'scorpion', 'crab', 'lobster', 'beetle', 'bug', 'insect', 'cockroach', 'spiderbot', 'spider bot'], 'spider', 'creature', 'kinematic', 'ground_front', { mach: { loco: 'legs', speed: 8, pet: true, melee: 'jaws', legs: 8 } }),
  C('shark', ['fish', 'dolphin', 'whale', 'orca', 'piranha', 'swordfish', 'tuna', 'salmon', 'goldfish', 'barracuda', 'megalodon', 'manta', 'stingray'], 'fish', 'creature', 'kinematic', 'ground_front', { mach: { loco: 'swim', speed: 10, altitude: 1.4, pet: true, melee: 'jaws' } }),
  // --- misc shapes
  C('spike', ['spikes', 'thorn', 'stake', 'caltrops', 'spike trap', 'bed of nails'], 'spike', 'weapon', 'static', 'aim'),
  C('star', ['starfish'], 'star', 'misc', 'kinematic', 'hand', { verb: 'strike' }),
  C('bolt', ['lightning', 'lightning bolt', 'thunderbolt', 'zap'], 'bolt', 'projectile', 'kinematic', 'hand', { verb: 'strike' }),
  C('pipe organ', ['xylophone'], 'tube', 'misc', 'static', 'aim'),
  C('chain', ['chains', 'shackle chain', 'anchor chain', 'bike chain'], 'chain', 'rope', 'kinematic', 'hand', { verb: 'pull' }),
];

/** Add-on parts ("with rockets on the back"). Each contributes geometry + behaviour. */
export type MachineKind = 'heads' | 'wheels' | 'treads' | 'legs' | 'wings' | 'rotor' | 'propeller' | 'jets' | 'hover' | 'turret' | 'gun' | 'jaws' | 'arms' | 'claws' | 'tentacles' | 'tail' | 'seat' | 'melee' | 'saw' | 'drill';
export type WeaponKind = 'gun' | 'cannon' | 'laser' | 'missile' | 'flame' | 'net' | 'melee' | 'bomb';
export interface Addon {
  id: string; names: string[]; form: string; where: 'back' | 'front' | 'top' | 'bottom' | 'sides' | 'around' | 'tip' | 'head'; scale: number; beh?: any[]; mountMods?: Record<string, number>; density?: number; restitution?: number;
  /** articulated machine part kit (built by machines.ts: parts + joints + drive/weapon/rideable) */
  machine?: MachineKind;
  weapon?: WeaponKind;
  /** machine kit adds behaviour only; geometry comes from `form` as for plain add-ons */
  keepGeometry?: boolean;
}
export const ADDONS: Addon[] = [
  { id: 'rocket', names: ['rocket', 'rockets', 'thruster', 'thrusters', 'jet', 'jets', 'booster', 'boosters', 'engine', 'engines', 'rocket engine', 'rocket engines', 'afterburner', 'afterburners', 'jet engine', 'jet engines', 'rocket booster', 'rocket boosters'], form: 'thruster', where: 'back', scale: 0.9, beh: [{ type: 'thrust', accel: 25, axis: [0, 0, 1] }] },
  { id: 'wheel', names: ['wheel', 'wheels', 'tire', 'tires', 'tyre', 'tyres', 'rollers', 'casters', 'monster truck wheels', 'monster wheels'], form: 'm_wheel', where: 'bottom', scale: 0.35, mountMods: { groundSpeedMul: 1.8, frictionMul: 0.4 }, machine: 'wheels' },
  { id: 'tread', names: ['tread', 'treads', 'tracks', 'tank treads', 'tank tracks', 'caterpillar tracks', 'caterpillar treads', 'crawler tracks'], form: 'm_wheel', where: 'bottom', scale: 0.35, mountMods: { groundSpeedMul: 1.4, frictionMul: 0.6 }, machine: 'treads' },
  { id: 'wing', names: ['wing', 'wings', 'fins', 'fin', 'sails', 'feathers', 'bat wings', 'angel wings', 'dragon wings', 'bird wings'], form: 'wings', where: 'sides', scale: 0.8, mountMods: { glide: 1.2 }, machine: 'wings' },
  { id: 'spike', names: ['spike', 'spikes', 'spiky bits', 'studs', 'thorns', 'barbs', 'nails', 'horns', 'horn'], form: 'spike', where: 'front', scale: 0.5, density: 1.5, machine: 'melee', keepGeometry: true },
  { id: 'blade', names: ['blade', 'blades', 'knives', 'swords', 'razors', 'edges', 'scythes'], form: 'sword', where: 'sides', scale: 0.5, machine: 'melee', keepGeometry: true },
  { id: 'saw', names: ['saw', 'saws', 'saw blade', 'saw blades', 'buzzsaw', 'buzzsaws', 'buzz saw', 'buzz saws', 'circular saw', 'circular saws', 'chainsaw', 'chainsaws'], form: 'saw', where: 'front', scale: 0.5, machine: 'saw' },
  { id: 'propeller', names: ['propeller', 'propellers', 'prop', 'props', 'propellor', 'propellors'], form: 'm_prop', where: 'top', scale: 0.8, mountMods: { thrustUp: 24 }, machine: 'propeller' },
  { id: 'rotor', names: ['rotor', 'rotors', 'helicopter blades', 'helicopter blade', 'heli blades', 'chopper blades', 'rotor blades', 'helicopter rotor', 'helicopter rotors', 'helicopter propeller', 'helicopter propellers'], form: 'm_rotor', where: 'top', scale: 0.8, mountMods: { thrustUp: 24 }, machine: 'rotor' },
  { id: 'magnet', names: ['magnet', 'magnets'], form: 'magnet', where: 'front', scale: 0.5, beh: [{ type: 'field', mode: 'attract', radius: 12, strength: 30 }] },
  { id: 'hook', names: ['hook', 'hooks', 'grappling hook', 'grapples'], form: 'hook', where: 'front', scale: 0.8 },
  { id: 'claw', names: ['claw', 'claws', 'pincer', 'pincers', 'nippers', 'talons', 'crab claws', 'grabber claws', 'robot claws'], form: 'claw', where: 'front', scale: 0.5, machine: 'claws' },
  { id: 'cannon', names: ['cannon', 'cannons', 'artillery', 'big gun', 'big guns', 'howitzer', 'mortar'], form: 'm_barrel', where: 'top', scale: 0.8, machine: 'gun', weapon: 'cannon' },
  { id: 'gun', names: ['gun', 'guns', 'machine gun', 'machine guns', 'machinegun', 'minigun', 'miniguns', 'gatling', 'gatling gun', 'gatling guns', 'rifle', 'rifles', 'blaster', 'blasters', 'pistol', 'pistols', 'shotgun', 'shotguns', 'chaingun'], form: 'm_barrel', where: 'top', scale: 0.8, machine: 'gun', weapon: 'gun' },
  { id: 'turret', names: ['turret', 'turrets', 'gun turret', 'gun turrets', 'cannon turret', 'tank turret'], form: 'm_barrel', where: 'top', scale: 0.8, machine: 'turret', weapon: 'cannon' },
  { id: 'laser', names: ['laser', 'lasers', 'laser gun', 'laser guns', 'laser cannon', 'laser cannons', 'laser beam', 'laser beams', 'ray gun', 'ray guns', 'death ray', 'lazer', 'lazers', 'laser eyes', 'eye lasers', 'beam cannon'], form: 'm_barrel', where: 'top', scale: 0.8, machine: 'gun', weapon: 'laser' },
  { id: 'missile', names: ['missile', 'missiles', 'missile launcher', 'missile launchers', 'rocket launcher', 'rocket launchers', 'missile pod', 'missile pods', 'homing missiles', 'torpedo', 'torpedoes', 'rpg', 'rpgs'], form: 'm_barrel', where: 'top', scale: 0.8, machine: 'gun', weapon: 'missile' },
  { id: 'flamethrower', names: ['flamethrower', 'flamethrowers', 'flame thrower', 'flame throwers', 'flamer', 'fire breath', 'flame breath', 'fire cannon', 'flame cannon'], form: 'm_barrel', where: 'front', scale: 0.8, machine: 'gun', weapon: 'flame' },
  { id: 'netgun', names: ['net gun', 'net guns', 'net launcher', 'net launchers', 'net cannon', 'net cannons', 'web shooter', 'web shooters'], form: 'm_barrel', where: 'top', scale: 0.8, machine: 'gun', weapon: 'net' },
  { id: 'bomb', names: ['bomb bay', 'bombs', 'bomb', 'bomb launcher', 'grenade launcher', 'grenade launchers', 'grenades', 'depth charges'], form: 'm_barrel', where: 'bottom', scale: 0.8, machine: 'gun', weapon: 'bomb' },
  { id: 'fan', names: ['fan', 'fans', 'turbine', 'turbines'], form: 'fan', where: 'back', scale: 0.5, beh: [{ type: 'field', mode: 'wind', radius: 12, strength: 30 }] },
  { id: 'spring', names: ['spring', 'springs', 'coils', 'pogo', 'bounce'], form: 'spring', where: 'bottom', scale: 0.4, beh: [{ type: 'launch', speed: 24, dir: 'up' }], restitution: 0.9 },
  { id: 'handle', names: ['handle', 'handles', 'grip', 'grips', 'hilt'], form: 'pole', where: 'back', scale: 0.2 },
  { id: 'chain', names: ['chain', 'chains', 'rope', 'ropes', 'cable', 'cables', 'tether'], form: 'chain', where: 'back', scale: 0.8 },
  { id: 'leg', names: ['leg', 'legs', 'stilts', 'feet', 'spider legs', 'robot legs', 'mech legs', 'chicken legs', 'insect legs'], form: 'm_leg', where: 'bottom', scale: 0.3, machine: 'legs' },
  { id: 'finger', names: ['finger', 'fingers', 'thumb', 'thumbs', 'hand', 'hands'], form: 'hand', where: 'front', scale: 0.6 },
  { id: 'arm', names: ['arm', 'arms', 'fist', 'fists', 'robot arms', 'robotic arms', 'mechanical arms', 'punching arms', 'boxing gloves', 'gloves'], form: 'm_leg', where: 'sides', scale: 0.6, machine: 'arms' },
  { id: 'jaws', names: ['jaw', 'jaws', 'mouth', 'mouths', 'teeth', 'fangs', 'beak', 'chompers', 'maw', 'bite', 'shark teeth', 'big teeth', 'sharp teeth'], form: 'm_jaw', where: 'front', scale: 0.5, machine: 'jaws' },
  { id: 'tentacle', names: ['tentacle', 'tentacles', 'octopus arms', 'octopus legs', 'feelers'], form: 'm_segment', where: 'bottom', scale: 0.5, machine: 'tentacles' },
  { id: 'tail', names: ['tail', 'tails', 'scorpion tail', 'dragon tail', 'stinger'], form: 'm_segment', where: 'back', scale: 0.5, machine: 'tail' },
  { id: 'seat', names: ['saddle', 'saddles', 'seat', 'seats', 'cockpit', 'driver seat', 'drivers seat', 'chair', 'throne', 'passenger seat', 'bench seat'], form: 'm_seat', where: 'top', scale: 0.5, machine: 'seat' },
  { id: 'shield', names: ['shield', 'shields', 'armor', 'armour', 'plating', 'bumper', 'bumpers'], form: 'shield', where: 'front', scale: 0.6 },
  { id: 'parachute', names: ['parachute', 'parachutes', 'canopy', 'umbrella', 'balloon', 'balloons'], form: 'parachute', where: 'top', scale: 0.7, mountMods: { maxFall: 5 } },
  { id: 'light', names: ['light', 'lights', 'headlights', 'lamp', 'lamps', 'glow', 'stripes', 'stars', 'sparkles', 'smiley face', 'face', 'eyes', 'logo', 'flames', 'flame decals', 'racing stripes'], form: 'disc', where: 'front', scale: 0.25 },
  { id: 'net', names: ['net', 'nets', 'mesh'], form: 'net', where: 'front', scale: 0.5 },
  { id: 'bucket', names: ['bucket', 'basket', 'scoop', 'cup'], form: 'bucket', where: 'front', scale: 0.6 },
  { id: 'drill', names: ['drill', 'drills', 'drill bit', 'drill tip', 'drill nose'], form: 'drill', where: 'front', scale: 0.6, machine: 'drill' },
  { id: 'hammerhead', names: ['hammer', 'hammers', 'mallet'], form: 'hammer', where: 'front', scale: 0.6 },
  { id: 'head', names: ['head', 'heads', 'extra head', 'extra heads', 'second head', 'more heads'], form: 'ball', where: 'head', scale: 0.3, machine: 'heads' },
  { id: 'hover', names: ['hover pads', 'hover jets', 'hoverpads', 'antigravity', 'anti gravity', 'repulsors'], form: 'disc', where: 'bottom', scale: 0.3, machine: 'hover' },
];

/** Effects the construct substrate cannot represent at all — abstain visibly instead of faking them. */
export const UNSUPPORTED: { pattern: RegExp; reason: string }[] = [
  { pattern: /\b(invisible|invisibility)\b/, reason: 'constructs can’t make things invisible', strippable: true } as any,
  { pattern: /\b(make (me|him|her|it|them) (disappear|vanish))\b/, reason: 'constructs can’t make things invisible' },
  { pattern: /\b(stop|freeze|pause|rewind|reverse|slow down|turn back|speed up|go back in|travel back in|bend) (the )?time\b|\btime travel\b|\bslow ?mo(tion)?\b|\brewind\b|\bpause the world\b/, reason: 'the ring can’t bend time' },
  { pattern: /\b(teleport|teleports|teleportation|blink me)\b/, reason: 'no teleportation — try a launcher or a bridge' },
  { pattern: /\b(heal|cure|revive|resurrect|fix (him|her|me|them))\b/, reason: 'constructs can’t heal' },
  { pattern: /\b(read (his|her|their) mind|mind control|hypnoti[sz]e|make (him|her|them) (like|love) me)\b/, reason: 'no mind effects' },
  { pattern: /\b(real|actual) (fire|water|lava|ice|electricity|lightning)\b/, reason: 'hard light is solid light — no real elements' },
  { pattern: /\b(make it rain|weather|sunshine|earthquake|volcano)\b/, reason: 'can’t change weather or terrain' },
  { pattern: /\b(clone me|another me|copy of me|duplicate me)\b/, reason: 'can’t copy people' },
  { pattern: /\b(delete the (world|level|map)|end the game|destroy (the )?(world|universe|planet)|kill everyone)\b/, reason: 'out of bounds for the ring' },
  { pattern: /\b(transform|turn) (me|myself) into\b/, reason: 'the ring shapes light, not you' },
  { pattern: /\b(shrink me|grow me|make me (bigger|smaller|giant|tiny|huge))\b/, reason: 'can’t resize you — try a construct instead' },
  { pattern: /\b(explode|blow up) (myself|me)\b/, reason: 'the ring won’t blow you up' },
];

// ---------------------------------------------------------------- index

export const conceptByName = new Map<string, Concept>();
for (const c of CONCEPTS) for (const n of c.names) if (!conceptByName.has(n)) conceptByName.set(n, c);
export const addonByName = new Map<string, Addon>();
for (const a of ADDONS) for (const n of a.names) if (!addonByName.has(n)) addonByName.set(n, a);
export const MAX_NAME_WORDS = Math.max(...[...conceptByName.keys(), ...addonByName.keys()].map((n) => n.split(' ').length));
