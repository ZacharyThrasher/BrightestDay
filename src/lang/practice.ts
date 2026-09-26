/**
 * Practice duel wiring: a second lantern driven by a bot, typing through its OWN Director (the same System-1
 * path a human uses, committed synchronously inside the deterministic tick).
 *
 *   const { player, director, bot } = attachPracticeBot(world);   // 'p2' vs the local 'p1'
 *   world.startMatch();                                             // first to 3, warmup, respawns…
 *
 * A smarter rival only needs another BotController (src/sim/bots.ts) — pass it as `controller`.
 */
import type { World } from '../sim/world.js';
import type { Player } from '../sim/player.js';
import { PracticeBot, type BotController, type PracticeBotOpts } from '../sim/bots.js';
import { Director } from './director.js';

export interface BotHandle { player: Player; director: Director; bot: BotController }

export function attachPracticeBot(
  world: World,
  opts: { id?: string; name?: string; bot?: Partial<PracticeBotOpts>; controller?: (id: string, director: Director) => BotController } = {},
): BotHandle {
  const id = opts.id ?? 'p2';
  const player = world.addPlayer(id, opts.name ?? 'practice bot');
  const director = new Director(world, id);
  const sink = { submit: (text: string) => director.commitSync(text) };
  const bot = opts.controller ? opts.controller(id, director) : new PracticeBot(id, sink, opts.bot);
  world.controllers.set(id, bot);
  return { player, director, bot };
}
