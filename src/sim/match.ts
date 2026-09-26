/**
 * Duel rules (docs/PVP_DESIGN.md "Match rules"). Deterministic: driven only by world time and world events,
 * no wall clock, no RNG; the whole state is a plain serializable object (`snapshot()` / `restore()`), so a host
 * can run it authoritatively and ship it to clients.
 *
 *   warmup (3 s countdown, no damage) → live → round_end (a short result screen) → next round's warmup, or
 *   match_end once someone has won enough rounds (default: one round = the match).
 *   A round is won by the first kill to `killsToWin` (3). After `roundTime` (3 min) the leader wins the round;
 *   when tied it goes to sudden death (the next kill wins).
 *
 * Events (world.emit):
 *   'match_state' { phase, round, scores, rounds, winner, suddenDeath, endsAt }
 *   'score'       { scores, player, by }
 *   'kill'        { killer, victim, weapon }   (weapon = the construct's label, or the cause)
 */
import type { World, GameEvent } from './world.js';

export type MatchPhase = 'warmup' | 'live' | 'round_end' | 'match_end';

export interface MatchRules {
  killsToWin: number;
  roundTime: number;
  roundsToWin: number;
  warmup: number;
  roundEndTime: number;
  respawnDelay: number;
  spawnProtection: number;
}

export const DUEL_RULES: MatchRules = { killsToWin: 3, roundTime: 180, roundsToWin: 1, warmup: 3, roundEndTime: 3, respawnDelay: 3, spawnProtection: 2 };

export interface MatchSnapshot {
  rules: MatchRules;
  players: string[];
  phase: MatchPhase;
  phaseAt: number;
  round: number;
  roundStartedAt: number;
  scores: Record<string, number>;
  rounds: Record<string, number>;
  suddenDeath: boolean;
  roundWinner: string | null;
  winner: string | null;
  kills: { t: number; killer: string | null; victim: string; weapon: string }[];
}

export class Match {
  rules: MatchRules;
  players: string[];
  phase: MatchPhase = 'warmup';
  phaseAt = 0;
  round = 1;
  roundStartedAt = 0;
  scores: Record<string, number> = {};
  rounds: Record<string, number> = {};
  suddenDeath = false;
  roundWinner: string | null = null;
  winner: string | null = null;
  /** kill log of the match (kill feed, stats) */
  kills: MatchSnapshot['kills'] = [];

  constructor(private w: World, players: string[], rules: Partial<MatchRules> = {}) {
    this.rules = { ...DUEL_RULES, ...rules };
    this.players = players.slice();
    for (const p of this.players) { this.scores[p] = 0; this.rounds[p] = 0; }
    this.enter('warmup');
  }

  /** Damage (and kills) only count while the round is live. */
  get damageEnabled() { return this.phase === 'live'; }
  /** Seconds left in the current phase (warmup countdown, round clock, result screen); Infinity when open-ended. */
  get timeLeft(): number {
    const t = this.w.time;
    switch (this.phase) {
      case 'warmup': return Math.max(0, this.phaseAt + this.rules.warmup - t);
      case 'live': return this.suddenDeath ? Infinity : Math.max(0, this.roundStartedAt + this.rules.roundTime - t);
      case 'round_end': return Math.max(0, this.phaseAt + this.rules.roundEndTime - t);
      default: return Infinity;
    }
  }

  private enter(phase: MatchPhase) {
    this.phase = phase;
    this.phaseAt = this.w.time;
    if (phase === 'live') this.roundStartedAt = this.w.time;
    this.w.emit('match_state', this.stateEvent());
  }

  stateEvent() {
    return { phase: this.phase, round: this.round, scores: { ...this.scores }, rounds: { ...this.rounds }, winner: this.winner, roundWinner: this.roundWinner, suddenDeath: this.suddenDeath, endsAt: isFinite(this.timeLeft) ? this.w.time + this.timeLeft : null };
  }

  /** World events: kills score. Called by the world for every event. */
  onEvent(e: GameEvent) {
    if (e.type !== 'player_killed' || this.phase !== 'live') return;
    const victim = String(e.target);
    const killer: string | null = e.attacker && e.attacker !== victim ? String(e.attacker) : null;
    const weapon = String(e.weapon ?? e.cause ?? 'the ring');
    this.kills.push({ t: this.w.time, killer, victim, weapon });
    if (this.kills.length > 64) this.kills.shift();
    this.w.emit('kill', { killer, victim, weapon });
    // a suicide / environment kill scores for the other side in a duel
    const scorer = killer ?? (this.players.length === 2 ? this.players.find((p) => p !== victim) ?? null : null);
    if (scorer && scorer in this.scores) {
      this.scores[scorer]++;
      this.w.emit('score', { scores: { ...this.scores }, player: scorer, by: killer });
      if (this.scores[scorer] >= this.rules.killsToWin || this.suddenDeath) this.endRound(scorer);
    }
  }

  /** Per tick (after the simulation step). */
  update() {
    const t = this.w.time;
    switch (this.phase) {
      case 'warmup':
        if (t >= this.phaseAt + this.rules.warmup) this.enter('live');
        break;
      case 'live':
        if (!this.suddenDeath && t >= this.roundStartedAt + this.rules.roundTime) {
          const best = this.leader();
          if (best) this.endRound(best);
          else { this.suddenDeath = true; this.w.emit('match_state', this.stateEvent()); }
        }
        break;
      case 'round_end':
        if (t >= this.phaseAt + this.rules.roundEndTime) {
          if (this.winner) this.enter('match_end');
          else this.nextRound();
        }
        break;
    }
  }

  private leader(): string | null {
    let best: string | null = null, bs = -1, tie = false;
    for (const p of this.players) {
      const s = this.scores[p] ?? 0;
      if (s > bs) { bs = s; best = p; tie = false; } else if (s === bs) tie = true;
    }
    return tie ? null : best;
  }

  private endRound(winner: string) {
    this.roundWinner = winner;
    this.rounds[winner] = (this.rounds[winner] ?? 0) + 1;
    if (this.rounds[winner] >= this.rules.roundsToWin) this.winner = winner;
    this.enter('round_end');
  }

  private nextRound() {
    this.round++;
    for (const p of this.players) this.scores[p] = 0;
    this.suddenDeath = false;
    this.roundWinner = null;
    this.w.resetForRound();
    this.enter('warmup');
  }

  /** Start over (same players, same rules): scores cleared, everyone respawned, constructs dissolved. */
  rematch() {
    this.round = 1;
    for (const p of this.players) { this.scores[p] = 0; this.rounds[p] = 0; }
    this.suddenDeath = false;
    this.roundWinner = null;
    this.winner = null;
    this.kills = [];
    this.w.resetForRound();
    this.enter('warmup');
  }

  snapshot(): MatchSnapshot {
    return {
      rules: { ...this.rules }, players: this.players.slice(), phase: this.phase, phaseAt: this.phaseAt, round: this.round, roundStartedAt: this.roundStartedAt,
      scores: { ...this.scores }, rounds: { ...this.rounds }, suddenDeath: this.suddenDeath, roundWinner: this.roundWinner, winner: this.winner, kills: this.kills.map((k) => ({ ...k })),
    };
  }

  restore(s: MatchSnapshot) {
    Object.assign(this, { rules: { ...s.rules }, players: s.players.slice(), phase: s.phase, phaseAt: s.phaseAt, round: s.round, roundStartedAt: s.roundStartedAt, scores: { ...s.scores }, rounds: { ...s.rounds }, suddenDeath: s.suddenDeath, roundWinner: s.roundWinner, winner: s.winner, kills: s.kills.map((k) => ({ ...k })) });
  }
}
