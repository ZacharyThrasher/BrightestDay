/**
 * Movement tuning. Units: metres, seconds.
 * Everything lives here so feel can be iterated without touching logic
 * (and so a player can override it from settings.json).
 */
export const MOVE = {
  gravity: 22,
  // ground
  runSpeed: 11,
  groundAccel: 10, // Source-style: accel * wishspeed * dt
  friction: 5,
  stopSpeed: 3,
  jumpSpeed: 10,
  jumpBuffer: 0.12, // jump pressed this long before landing still counts (no friction on that tick)
  coyoteTime: 0.1,
  groundMaxVy: 3.5, // moving up faster than this relative to ground => airborne (ramp launches keep speed)
  // slide (crouch at speed)
  slideMinSpeed: 7,
  slideFriction: 0.25,
  slideGravityAlongSlope: 1.0,
  // air
  airAccel: 14,
  airWishCap: 1.4, // Source's 30u cap scaled: permits air-strafe speed gain
  // boost (hard-light thruster, hold)
  boostAccel: 34,
  boostDrain: 32,
  gasMax: 100,
  gasRegenGround: 40,
  gasRegenAir: 10,
  // tethers (AoTTG-inspired)
  tetherRange: 140,
  tetherShotSpeed: 320,
  tetherPull: 16, // passive pull toward anchor
  tetherReelPull: 58, // extra pull while reeling
  tetherReelRate: 26, // rope shortening m/s while reeling
  // flight (accessible, deliberately slower than momentum tech)
  flyCruise: 17,
  flyAccel: 22,
  flyOverspeedDrag: 0.12, // fraction of excess speed removed per second above cruise
  flyDiveGravity: 0.65, // fraction of gravity applied along a dive
  // PvP speed ceiling: below softCap nothing changes (the feel is untouched). Above it, a drag that grows
  // with the excess (linear + quadratic) bleeds stacked speed back down, so bursts still feel huge but
  // can't be sustained. Launches from constructs are exempt for a moment so they still land where aimed.
  softCap: 28,
  softCapDragLin: 0.5, // 1/s on the excess
  softCapDragQuad: 0.08, // 1/(m) on excess² (boost alone now peaks ≈ 45 m/s instead of ≈ 64)
  launchGrace: 1.6, // seconds of no soft-cap drag after a construct launches you
  // global
  maxSpeed: 70,
  playerRadius: 0.4,
  playerHalfHeight: 0.5, // capsule half-height (cylinder part) -> total 1.8 m
  playerMass: 80,
} as const;

export type MoveTuning = { -readonly [K in keyof typeof MOVE]: number };
