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
  C('turret', ['sentry', 'sentry gun', 'gun turret', 'auto turret', 'tank'], 'turret', 'weapon', 'static', 'aim', { beh: [{ type: 'shoot', target: { kind: 'nearest_enemy' }, rate: 2, speed: 50, size: 0.3 }], verb: 'shoot' }),
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
  C('car', ['truck', 'van', 'bus', 'jeep', 'go kart', 'go-kart', 'kart', 'buggy', 'dune buggy', 'race car', 'racecar', 'motorcycle', 'motorbike', 'bike', 'bicycle', 'scooter', 'segway', 'wagon', 'cart', 'shopping cart', 'wheelchair', 'monster truck', 'quad bike', 'atv', 'tricycle', 'unicycle', 'train', 'locomotive', 'tram', 'trolley', 'minecart', 'mine cart', 'sled', 'sleigh', 'bobsled', 'toboggan', 'chariot', 'rickshaw', 'lawnmower', 'lawn mower', 'forklift', 'tractor', 'bulldozer car', 'snowmobile'], 'car', 'vehicle', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'ride', groundSpeedMul: 2.4, frictionMul: 0.4 }] }),
  C('hamster ball', ['zorb', 'zorb ball', 'rolling ball', 'giant hamster ball'], 'bubble', 'vehicle', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'ride', groundSpeedMul: 2.2, frictionMul: 0.3 }] }),
  C('skateboard', ['longboard', 'roller skates', 'rollerblades', 'skates', 'ice skates', 'skis', 'ski', 'snowboard'], 'skateboard', 'vehicle', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'ride', groundSpeedMul: 2.0, frictionMul: 0.15 }] }),
  C('hoverboard', ['surfboard', 'flying surfboard', 'surf board', 'hover board', 'magic carpet', 'carpet', 'flying carpet', 'broomstick', 'flying broom', 'hover disc', 'flying saucer', 'ufo', 'hover bike', 'hoverbike', 'jet ski', 'jetski', 'cloud', 'nimbus'], 'surfboard', 'vehicle', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'ride', glide: 1.6, thrustLook: 14, maxFall: 10 }] }),
  C('wings', ['wing', 'wingsuit', 'wing suit', 'angel wings', 'bat wings', 'butterfly wings', 'feathers', 'cape', 'hang glider', 'kite'], 'wings', 'wearable', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'wear', glide: 1.3 }] }),
  C('glider', ['paraglider', 'sailplane', 'glider wing'], 'glider', 'wearable', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'wear', glide: 1.6, maxFall: 12 }] }),
  C('parachute', ['chute', 'parasail', 'balloon', 'hot air balloon', 'balloons', 'bunch of balloons', 'blimp', 'zeppelin', 'feather', 'dandelion'], 'parachute', 'wearable', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'wear', maxFall: 4 }] }),
  C('jetpack', ['jet pack', 'rocket pack', 'rocket boots', 'jet boots', 'thrusters', 'rocket shoes', 'rocketpack', 'backpack rockets', 'booster pack'], 'jetpack', 'wearable', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'wear', thrustUp: 34, glide: 0.3 }] }),
  C('pogo boots', ['spring shoes', 'spring boots', 'moon boots', 'bouncy shoes', 'kangaroo boots', 'spring legs'], 'spring', 'wearable', 'kinematic', 'self', { scale: 0.35, beh: [{ type: 'mount', mode: 'wear', bounce: 16 }] }),
  C('plane', ['airplane', 'aeroplane', 'jet', 'fighter jet', 'biplane', 'airliner', 'paper airplane', 'paper plane', 'spaceship', 'space ship', 'starship', 'shuttle', 'space shuttle', 'bird plane'], 'plane', 'vehicle', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'ride', glide: 1.8, thrustLook: 18 }] }),
  C('helicopter', ['chopper', 'drone ship', 'quadcopter', 'gyrocopter', 'propeller hat', 'propeller beanie'], 'helicopter', 'vehicle', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'ride', thrustUp: 30, maxFall: 8 }] }),
  C('boat', ['canoe', 'kayak', 'rowboat', 'sailboat', 'ship', 'yacht', 'submarine', 'surf raft', 'dinghy', 'ark', 'gondola'], 'boat', 'vehicle', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'ride', groundSpeedMul: 1.8, frictionMul: 0.3, glide: 0.5 }] }),
  // --- creatures (approximations; visibly flagged as approximated by the parser)
  C('dragon', ['wyvern', 'phoenix', 'griffin', 'gryphon', 'pegasus', 'pterodactyl', 'eagle ride'], 'dragon', 'creature', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'ride', glide: 2, thrustLook: 16, thrustUp: 20 }] }),
  C('bird', ['eagle', 'hawk', 'falcon', 'crow', 'raven', 'owl', 'pigeon', 'bat creature', 'bee', 'wasp', 'hornet', 'mosquito', 'fly'], 'bird', 'creature', 'kinematic', 'hand', { verb: 'strike' }),
  C('horse', ['pony', 'unicorn', 'camel', 'elephant', 'rhino', 'rhinoceros', 'bull', 'ox', 'dinosaur', 't-rex', 'trex', 'mech', 'robot', 'giant robot', 'golem'], 'horse', 'creature', 'kinematic', 'self', { beh: [{ type: 'mount', mode: 'ride', groundSpeedMul: 2.2, frictionMul: 0.5 }] }),
  C('snake', ['serpent', 'python', 'anaconda', 'boa', 'worm', 'eel', 'octopus', 'squid', 'kraken', 'centipede', 'caterpillar'], 'snake', 'creature', 'kinematic', 'around_target', { beh: [{ type: 'restrain', target: T }], verb: 'trap' }),
  C('dog', ['hound', 'wolf', 'puppy', 'cat', 'tiger', 'lion', 'bear', 'gorilla', 'monkey', 'shark', 'crocodile', 'alligator', 'fox', 'panther', 'raptor'], 'creature', 'creature', 'kinematic', 'hand', { verb: 'grab' }),
  // --- misc shapes
  C('spike', ['spikes', 'thorn', 'stake', 'caltrops', 'spike trap', 'bed of nails'], 'spike', 'weapon', 'static', 'aim'),
  C('star', ['starfish'], 'star', 'misc', 'kinematic', 'hand', { verb: 'strike' }),
  C('bolt', ['lightning', 'lightning bolt', 'thunderbolt', 'zap'], 'bolt', 'projectile', 'kinematic', 'hand', { verb: 'strike' }),
  C('pipe organ', ['xylophone'], 'tube', 'misc', 'static', 'aim'),
  C('chain', ['chains', 'shackle chain', 'anchor chain', 'bike chain'], 'chain', 'rope', 'kinematic', 'hand', { verb: 'pull' }),
];

/** Add-on parts ("with rockets on the back"). Each contributes geometry + behaviour. */
export interface Addon { id: string; names: string[]; form: string; where: 'back' | 'front' | 'top' | 'bottom' | 'sides' | 'around' | 'tip'; scale: number; beh?: any[]; mountMods?: Record<string, number>; density?: number; restitution?: number }
export const ADDONS: Addon[] = [
  { id: 'rocket', names: ['rocket', 'rockets', 'thruster', 'thrusters', 'jet', 'jets', 'booster', 'boosters', 'engine', 'engines', 'rocket engine', 'rocket engines', 'afterburner', 'afterburners', 'jet engine', 'jet engines', 'rocket booster', 'rocket boosters'], form: 'thruster', where: 'back', scale: 0.9, beh: [{ type: 'thrust', accel: 25, axis: [0, 0, 1] }] },
  { id: 'wheel', names: ['wheel', 'wheels', 'tire', 'tires', 'tyres', 'rollers', 'casters'], form: 'wheel', where: 'bottom', scale: 0.35, mountMods: { groundSpeedMul: 1.8, frictionMul: 0.4 } },
  { id: 'wing', names: ['wing', 'wings', 'fins', 'fin', 'sails', 'feathers'], form: 'wings', where: 'sides', scale: 0.8, mountMods: { glide: 1.2 } },
  { id: 'spike', names: ['spike', 'spikes', 'spiky bits', 'studs', 'thorns', 'teeth', 'barbs', 'nails', 'horns', 'horn'], form: 'spike', where: 'front', scale: 0.5, density: 1.5 },
  { id: 'blade', names: ['blade', 'blades', 'knives', 'swords', 'saw blades', 'saws', 'razors', 'edges'], form: 'sword', where: 'sides', scale: 0.5 },
  { id: 'propeller', names: ['propeller', 'propellers', 'rotor', 'rotors', 'prop', 'props', 'helicopter blades'], form: 'propeller', where: 'top', scale: 0.8, beh: [{ type: 'spin', rate: 20, axis: [0, 1, 0] }], mountMods: { thrustUp: 24 } },
  { id: 'magnet', names: ['magnet', 'magnets'], form: 'magnet', where: 'front', scale: 0.5, beh: [{ type: 'field', mode: 'attract', radius: 12, strength: 30 }] },
  { id: 'hook', names: ['hook', 'hooks', 'grappling hook', 'grapples', 'claws', 'claw'], form: 'hook', where: 'front', scale: 0.8 },
  { id: 'cannon', names: ['cannon', 'cannons', 'gun', 'guns', 'turret', 'turrets', 'laser', 'lasers', 'blaster', 'blasters'], form: 'blaster', where: 'top', scale: 0.8, beh: [{ type: 'shoot', target: { kind: 'nearest_enemy' }, rate: 2, speed: 50, size: 0.3 }] },
  { id: 'fan', names: ['fan', 'fans', 'turbine', 'turbines'], form: 'fan', where: 'back', scale: 0.5, beh: [{ type: 'field', mode: 'wind', radius: 12, strength: 30 }] },
  { id: 'spring', names: ['spring', 'springs', 'coils', 'pogo', 'bounce'], form: 'spring', where: 'bottom', scale: 0.4, beh: [{ type: 'launch', speed: 24, dir: 'up' }], restitution: 0.9 },
  { id: 'handle', names: ['handle', 'handles', 'grip', 'grips', 'hilt'], form: 'pole', where: 'back', scale: 0.2 },
  { id: 'chain', names: ['chain', 'chains', 'rope', 'ropes', 'cable', 'cables', 'tether'], form: 'chain', where: 'back', scale: 0.8 },
  { id: 'leg', names: ['leg', 'legs', 'stilts', 'feet'], form: 'pole', where: 'bottom', scale: 0.3 },
  { id: 'finger', names: ['finger', 'fingers', 'thumb', 'thumbs', 'hand', 'hands', 'fist', 'fists', 'arm', 'arms'], form: 'hand', where: 'front', scale: 0.6 },
  { id: 'shield', names: ['shield', 'shields', 'armor', 'armour', 'plating', 'bumper', 'bumpers'], form: 'shield', where: 'front', scale: 0.6 },
  { id: 'parachute', names: ['parachute', 'parachutes', 'canopy', 'umbrella', 'balloon', 'balloons'], form: 'parachute', where: 'top', scale: 0.7, mountMods: { maxFall: 5 } },
  { id: 'light', names: ['light', 'lights', 'headlights', 'lamp', 'lamps', 'glow', 'stripes', 'stars', 'sparkles', 'smiley face', 'face', 'eyes', 'logo', 'flames', 'flame decals', 'racing stripes'], form: 'disc', where: 'front', scale: 0.25 },
  { id: 'net', names: ['net', 'nets', 'mesh'], form: 'net', where: 'front', scale: 0.5 },
  { id: 'bucket', names: ['bucket', 'basket', 'scoop', 'cup'], form: 'bucket', where: 'front', scale: 0.6 },
  { id: 'drill', names: ['drill', 'drills', 'drill bit', 'drill tip'], form: 'drill', where: 'front', scale: 0.6, beh: [{ type: 'spin', rate: 20, axis: [0, 0, 1] }] },
  { id: 'hammerhead', names: ['hammer', 'hammers', 'mallet'], form: 'hammer', where: 'front', scale: 0.6 },
];

/** Effects the construct substrate cannot represent at all — abstain visibly instead of faking them. */
export const UNSUPPORTED: { pattern: RegExp; reason: string }[] = [
  { pattern: /\b(invisible|invisibility|disappear|vanish)\b/, reason: 'constructs can’t make things invisible' },
  { pattern: /\b(time|rewind|slow ?mo|slow motion|pause the world|stop time|turn back)\b.*\b(time|clock)?\b/, reason: 'the ring can’t bend time' },
  { pattern: /\b(teleport|teleports|teleportation|blink me)\b/, reason: 'no teleportation — try a launcher or a bridge' },
  { pattern: /\b(heal|cure|revive|resurrect|fix (him|her|me|them))\b/, reason: 'constructs can’t heal' },
  { pattern: /\b(read (his|her|their) mind|mind control|hypnoti[sz]e|make (him|her|them) (like|love) me)\b/, reason: 'no mind effects' },
  { pattern: /\b(real|actual) (fire|water|lava|ice|electricity|lightning)\b/, reason: 'hard light is solid light — no real elements' },
  { pattern: /\b(make it rain|weather|sunshine|earthquake|volcano)\b/, reason: 'can’t change weather or terrain' },
  { pattern: /\b(clone me|another me|copy of me|duplicate me)\b/, reason: 'can’t copy people' },
  { pattern: /\b(delete the (world|level|map)|end the game|destroy (the )?(world|universe|planet)|kill everyone)\b/, reason: 'out of bounds for the ring' },
  { pattern: /\b(transform|turn) (me|myself) into\b/, reason: 'the ring shapes light, not you' },
  { pattern: /\b(shrink me|grow me|make me (bigger|smaller|giant|tiny|huge))\b/, reason: 'can’t resize you — try a construct instead' },
  { pattern: /\b(fly faster than light|speed of light|infinite (speed|power|energy))\b/, reason: 'no infinities' },
  { pattern: /\b(explode|blow up) (myself|me)\b/, reason: 'the ring won’t blow you up' },
];

// ---------------------------------------------------------------- index

export const conceptByName = new Map<string, Concept>();
for (const c of CONCEPTS) for (const n of c.names) if (!conceptByName.has(n)) conceptByName.set(n, c);
export const addonByName = new Map<string, Addon>();
for (const a of ADDONS) for (const n of a.names) if (!addonByName.has(n)) addonByName.set(n, a);
export const MAX_NAME_WORDS = Math.max(...[...conceptByName.keys(), ...addonByName.keys()].map((n) => n.split(' ').length));
