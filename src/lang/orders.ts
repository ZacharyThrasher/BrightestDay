/**
 * Orders to existing machines/creatures ("tank, attack that drone", "heel", "get in", "all of you follow me").
 * Pure token matching; the interpreter resolves addressees and targets against the scene.
 */
import type { BrainGoal } from './machines.js';

export type Order =
  | { kind: 'brain'; goal: BrainGoal; target: string[] | null; tag?: 'return' | 'fire' | 'aim' | 'stop'; strong: boolean; verb: string }
  | { kind: 'ride'; target: string[] | null; verb: string }
  | { kind: 'dismount'; verb: string };

const LEAD = new Set(['please', 'pls', 'plz', 'now', 'ok', 'okay', 'just', 'hey', 'yo', 'and', 'then', 'alright', 'right', 'quick', 'quickly', 'so', 'uh', 'um', 'oh']);
const TAIL = new Set(['please', 'pls', 'plz', 'now', 'thanks', 'thx', 'ty', 'quick', 'quickly', 'again', 'already', 'asap', 'boy', 'girl', 'buddy', 'mate', 'dude', 'bro', 'lol']);

export function cleanOrderTokens(toks: string[]): string[] {
  const t = toks.filter((w) => !/^[,.!?;:]+$/.test(w));
  while (t.length && LEAD.has(t[0])) t.shift();
  while (t.length && TAIL.has(t[t.length - 1])) t.pop();
  return t;
}

const OBJ = '(it|this|that|this one|that one|them|him|her|the [a-z]+( [a-z]+)?|my [a-z]+( [a-z]+)?|your [a-z]+|this [a-z]+|that [a-z]+)';

/** Match an order at the start of a (cleaned) clause. `strong` orders route to a machine even when it is not addressed by name. */
export function matchOrder(toksIn: string[]): Order | null {
  const toks = cleanOrderTokens(toksIn);
  if (!toks.length) return null;
  const s = toks.join(' ');
  let m: RegExpExecArray | null;
  // ---- riding
  if ((m = new RegExp(`^(get|hop|jump|climb|step|go)( back)? (in|on|into|onto|inside|aboard|on board|in to|on to)( ${OBJ})?$`).exec(s))) return { kind: 'ride', target: m[5] ? m[5].split(' ') : null, verb: 'get in' };
  if ((m = new RegExp(`^(ride|drive|pilot|fly|steer|board|mount|man|captain) ${OBJ}$`).exec(s))) return { kind: 'ride', target: m[2].split(' '), verb: m[1] };
  if (/^(ride|board|mount up|saddle up|all aboard)$/.test(s)) return { kind: 'ride', target: null, verb: s };
  if ((m = /^let me (drive|ride|fly|pilot|steer|in|on|get in|hop on|take the wheel|take over|control)( (it|this|that|them|the [a-z]+( [a-z]+)?))?$/.exec(s))) return { kind: 'ride', target: m[3] ? m[3].split(' ') : null, verb: 'let me ' + m[1] };
  if ((m = /^(i will|i wanna|i want to|i am going to|i am gonna|i gonna|i can|i will take the wheel|me) (drive|ride|fly|pilot|steer|take the wheel|take over|take control)( (it|this|that|the [a-z]+( [a-z]+)?))?$/.exec(s))) return { kind: 'ride', target: m[4] ? m[4].split(' ') : null, verb: m[2] };
  if (/^take (the )?(wheel|controls|stick|helm|reins)$/.test(s)) return { kind: 'ride', target: null, verb: 'take the wheel' };
  // ---- dismounting
  if (/^(get|hop|jump|climb|step|bail|bail out)( back)? (out|off)( of (it|this|that|the (?!way\b)[a-z]+))?$/.test(s) || /^(dismount|disembark|eject|exit|unmount|bail|bail out|let me (out|off)|stop (riding|driving|flying))( (it|this|that|the [a-z]+))?$/.test(s)) return { kind: 'dismount', verb: 'get out' };
  // ---- fire control
  if (/^(cease fire|hold (your )?fire|stop (shooting|firing))$/.test(s)) return { kind: 'brain', goal: 'idle', target: null, tag: 'stop', strong: true, verb: 'cease fire' };
  if ((m = /^(open fire|fire at will|fire away|fire|shoot|blast|light (him|her|them|it|em) up|let (it|them|em) rip|unload|pew pew|pew|engage|weapons free)( (at|on|upon))?( (.+))?$/.exec(s))) {
    const tgt = m[7] ? m[7].split(' ') : /light (him|her|them|it|em) up/.test(m[1]) ? [m[2]] : null;
    return { kind: 'brain', goal: 'hunt', target: tgt, tag: 'fire', strong: true, verb: 'fire' };
  }
  // ---- aiming
  if ((m = /^(turn|point|aim|rotate|swing|train|face|swivel|target|look)( (the|your|its|ur))? ?(turret|turrets|gun|guns|cannon|cannons|barrel|barrels|weapon|weapons|laser|lasers|head|it|them|missiles)?( (at|toward|towards|to|on))?( (.+))?$/.exec(s)) && (m[4] || m[6] || m[1] === 'target' || m[1] === 'aim') && !(m[1] === 'turn' && !m[4] && !m[6])) {
    const tgt = m[8] ? m[8].split(' ') : null;
    if (tgt && /^(around|into|vertical|upside|sideways|it|them)$/.test(tgt[0]) && !m[6]) return null;
    return { kind: 'brain', goal: 'idle', target: tgt, tag: 'aim', strong: !!m[4] && m[4] !== 'it' && m[4] !== 'them' && m[4] !== 'head', verb: 'aim' };
  }
  // ---- coming back / following
  if (/^((come|get|go|fly|run|drive|head|walk|swim|hurry) back( here| to me| home)?|return( to me| home)?|back to me|regroup|fall back|rtb|come home|get over here|come over here|over here|here)$/.test(s)) return { kind: 'brain', goal: 'follow', target: null, tag: 'return', strong: true, verb: 'come back' };
  if (/^(heel|come|come here|come on|come with me|come to me|come along|with me|to me|on me|stick with me|stick close|stay close|stay with me|keep up|follow|follow me|follow along|tag along|this way|walk with me|let us go|lets go|let's go)$/.test(s)) return { kind: 'brain', goal: 'follow', target: null, strong: true, verb: 'follow' };
  if ((m = /^(follow|escort|shadow|tail|accompany|track|stalk) (.+)$/.exec(s))) return { kind: 'brain', goal: m[2] === 'me' ? 'follow' : 'follow', target: m[2] === 'me' ? null : m[2].split(' '), strong: m[2] === 'me', verb: 'follow' };
  // ---- attacking
  if ((m = /^(sic|sick|sick em|sic em|get em|get them|go get em|go get them|go get him|go get her|attack|charge|kill|hunt|hunt down|destroy|chase|chase down|take out|take down|eliminate|go after|bite|eat|maul|smash|wreck|ram|fight|annihilate|obliterate|murder|get|go get|tear up|rip apart|crush|stomp|trample|sting|pounce on|jump)( (him|her|them|em|it|that|those|these|the .+|that .+|those .+|these .+|all .+|every .+|a .+|any .+|[a-z]+s))?( (.+))?$/.exec(s))) {
    const verb = m[1];
    const tgtStr = [m[3], m[5]].filter(Boolean).join(' ').trim();
    const target = tgtStr ? tgtStr.split(' ') : /\b(em|them)\b/.test(verb) ? ['them'] : /\b(him)\b/.test(verb) ? ['him'] : /\b(her)\b/.test(verb) ? ['her'] : null;
    if ((verb === 'get' || verb === 'go get' || verb === 'jump' || verb === 'crush' || verb === 'smash' || verb === 'eat') && !target) return null;
    if (verb === 'get' && target && /^(in|on|out|off|up|down|back|rid|me|us|across|over|a|an)$/.test(target[0])) return null;
    const strong = ['sic', 'sick', 'sick em', 'sic em', 'get em', 'go get em', 'go get them', 'go get him', 'go get her', 'attack', 'charge', 'kill', 'hunt', 'hunt down', 'chase', 'chase down', 'go after', 'bite', 'maul', 'eliminate', 'take out', 'take down', 'destroy', 'fight', 'sting'].includes(verb);
    return { kind: 'brain', goal: 'hunt', target, strong, verb };
  }
  // ---- guarding
  if ((m = /^(guard|protect|defend|watch|cover|keep watch( over)?|stand guard( over| at)?|keep an eye on|look after|watch over|keep (him|her|them|me) safe|keep safe)( (.+))?$/.exec(s))) {
    let tgt = m[6] ? m[6].split(' ') : null;
    if (/^keep (him|her|them|me) safe$/.test(m[1])) tgt = [m[1].split(' ')[1]];
    return { kind: 'brain', goal: 'guard', target: tgt, strong: true, verb: 'guard' };
  }
  // ---- fetching
  if ((m = /^(fetch|bring|go fetch|retrieve|get me|go get me|grab me|fetch me|bring me|pick up|collect|carry|haul|go grab|grab)( (me|us))?( (.+))?$/.exec(s))) {
    const verb = m[1];
    const raw = m[5] ? m[5].split(' ') : null;
    // "bring it back / over here" = come to me (handled elsewhere); "get me a car" = make one
    if (raw && (/^(a|an|some|another|two|three|four|five|back|\d+)$/.test(raw[0]) || /^(it|that|this|them)$/.test(raw[0]) && raw.some((w) => ['back', 'here', 'over'].includes(w)))) return null;
    const tgt = raw ? raw.filter((w) => !['back', 'here', 'over', 'to'].includes(w)) : null;
    if (tgt && !tgt.length) return null;
    if (/^(bring|carry|haul)$/.test(verb) && !tgt) return null;
    const strong = /^(fetch|go fetch|fetch me|retrieve|bring me|get me|go get me|grab me|go grab)$/.test(verb) || verb === 'bring' && !!m[3];
    return { kind: 'brain', goal: 'fetch', target: tgt && tgt.length ? tgt : null, strong, verb };
  }
  // ---- going places
  if ((m = /^(go|drive|move|head|fly|walk|run|roll|travel|race|zoom|swim|get|charge|advance|proceed|hurry|march)( over)? (to|there|here|towards?|over there|forward|ahead|that way|onward)( (.+))?$/.exec(s))) {
    const w = m[3];
    const tgt = m[5] ? m[5].split(' ') : w === 'there' || w === 'over there' || w === 'that way' ? ['there'] : w === 'here' ? ['here'] : w === 'forward' || w === 'ahead' || w === 'onward' ? ['forward'] : null;
    if (m[1] === 'get') return null;
    return { kind: 'brain', goal: 'goto', target: tgt, strong: true, verb: m[1] };
  }
  // ---- patrol / wander
  if ((m = /^(patrol|go patrol|patrol around)( (.+))?$/.exec(s))) return { kind: 'brain', goal: 'patrol', target: m[3] ? m[3].split(' ') : null, strong: true, verb: 'patrol' };
  if (/^(wander|wander around|wander off|roam|roam around|roam free|explore|go explore|go play|go have fun|play|go wild|free roam|run free|run around|mosey|be free|do your thing|off you go)$/.test(s)) return { kind: 'brain', goal: 'wander', target: null, strong: true, verb: 'wander' };
  // ---- stopping
  if (/^(stop|stay|sit|halt|freeze|wait|hold|hold it|hold position|hold still|stand still|stand down|stand by|park|rest|down|stay put|stay there|stay here|sit down|sit still|stop (it|that|moving|attacking|chasing|following( me)?|there)|chill|calm down|at ease|sleep|lie down|play dead|whoa|easy|good boy|good girl|enough|that is enough|thats enough|back off|leave it|drop it|stay back|stop right there|stop here|stop there|brake|stop the car)$/.test(s)) {
    if (/^(back off)$/.test(s)) return { kind: 'brain', goal: 'flee', target: null, strong: true, verb: 'back off' };
    return { kind: 'brain', goal: 'idle', target: null, tag: 'stop', strong: true, verb: toks[0] };
  }
  // ---- fleeing
  if (/^(run away|run|flee|retreat|escape|scram|get away|run for it|hide|shoo|get lost|bail|evacuate|get out of here|get outta here|save yourself|save yourselves|go hide)$/.test(s)) return { kind: 'brain', goal: 'flee', target: null, strong: true, verb: 'flee' };
  return null;
}

/** Words that name a whole group of the player's machines ("all of you", "everyone"). */
export const ALL_ADDRESS = /^(all of you|all of them|all you|you all|y'?all|yall|everyone|everybody|everything|all units|all|both of you|you two|you three|you guys|guys|team|squad|troops|units|boys|gang|army|minions)$/;
