# SFX wishlist (consolidated from all worker reports — generate in ONE sfxgen pass)

All procedural (tools/sfxgen), dry, carved per the bd_audio category rules, 3–6 variations where
frequent. Missing names are silent (callers already emit them).

| Cue | From | Notes |
|---|---|---|
| assemble_click | construct animation | per assembly stage; caller pitches up per stage; small hard-light "snap" |
| assemble_lock | construct animation | final stage lock (heavier clunk + shimmer) |
| hatch_open / hatch_close | construct animation | mechanical hinge; close = satisfying clunk |
| rotor_spinup | construct animation | rising whirr, loopable tail |
| exhaust_puff | construct animation | short pneumatic puff on acceleration |
| kaiju_step / footstep_heavy | construct animation | footfalls scaled by mass (sub thump + debris) |
| charge_loop / charge_full / charge_break / charge_release | charge | (sfxgen sounds_charge.py exists — verify complete) |
| shout_erupt / shout_slam / shout_release / shout_fizzle | shout words | (generated — verify wired to bd_fx::ShoutSfx) |
| air_jump | double jump | hard-light step pop |
| dodge | dodge | quick whoosh, pitch by direction optional |
| hook_fire / hook_attach / hook_reel_loop / hook_break / hook_slam | tracking/melee | thwip, taut rope, slam impact |
| lunge | melee assist | short committed dash whoosh |
| reveal_pulse | tracking | soft sonar-like ping for loud-action reveals |
| recall_blink | bounds/recall | hard-light blink-in at summoner |
| summon_ping | summon preview | tiny UI tick when the off-screen preview appears |
| alert_danger | signature cut-in | escalating alarm (severity param via pitch/gain) |
| domain_open / domain_hum_loop / domain_shatter | signature | dome erupt boom, inside drone, break |
| ult_charge / ult_fire | signature | beam ultimate windup + fire |
| strike_telegraph / strike_beam_loop | orbital strike | ground marker hum, downward beam |
| purple_travel_loop | erasing orb (Hollow Purple) | a huge sphere tearing the air: low roar + hiss, loopable, doppler-friendly; `SignatureCue` start/stop |
| erase_impact | erasing orb | something deleted: a hollow "vwump" + crumble (terrain / construct / player hit) |
| merge_flash | fusion windup | the blue + red orbs merging: a sucking inhale then a bright crack |
| legendary_sting | freedom bundle | short golden shimmer sting (hidden mechanic — no "reward" voice) |
| sputter | freedom bundle | flickering, underpowered cast |
| ads_in / ads_out | camera/ADS | subtle cloth/weapon raise |
| view_toggle | camera | very subtle UI whoosh |
| wall_refresh | wall refresh (air jump back) | tiny hard-light scuff tick on the wall contact |
| surf_loop | surfing | wind rush / hard-light hiss while riding a surf face, pitch + gain up with speed |
| amb_pool_drip | Backrooms poolrooms | sparse water drips + tiny lapping; needs a multi-storey `Water` zone in `EmitterMap::scan` (it is 2.5D: only the roof is scanned) to place it near `POOL_WATER` voxels |
| amb_fluoro_hum | Backrooms | 120 Hz fluorescent ballast buzz with a faint flicker tick (currently `amb_neon_hum` stands in) |
| amb_lava_bed | Ember | deep molten drone with slow bubbling (currently `amb_planet_hum` stands in); placed near `CliffEdge` shores |
| amb_lava_bubble | Ember | thick glooping pops / plops at lava shores (sparkle; `debris_patter` stands in) |
| amb_heat_roar | Ember | broadband heat shimmer / updraft roar at the lava edges (currently `wind_loop`) |
| amb_eruption_far | Ember | distant volcanic eruption: deep boom + long rumbling tail (currently `amb_distant_boom`) |
| amb_lavafall | Ember | continuous pouring / sizzling hiss of the lavafall under the fortress (needs a point emitter or an emissive-voxel zone in `EmitterMap::scan`) |
| amb_ember_crackle | Ember | faint crackle of embers on the wind (sparkle, Sky zone) |
| lava_ko | Ember / any lava void | sizzle + hiss burst when a body drops into the lava (client splash of the bottom blast zone on lava maps) |
| amb_sewer_drip | Turtle Lair | sparse drips + plinks near the sewer channels (sparkle); needs `EmitterMap::scan` to find `SEWER_WATER` voxels under the street (it only scans the top surface) |
| amb_sewer_trickle_loop | Turtle Lair | gentle running-water trickle loop for the channels and the hub waterfall (currently a quiet `amb_ocean_loop` bed stands in) |
| amb_subway_rumble | Turtle Lair | distant train rumble rolling through the chasm: low swell + rail clatter, 6–10 s (currently `amb_distant_boom`) |
| amb_pipe_hiss | Turtle Lair | steam-pipe hiss bursts + metallic clanks along the vault pipes (currently `amb_station_creak`) |
| amb_city_muffled | Turtle Lair | the city above heard through a manhole: low-passed traffic, a far siren (bed near the shafts; currently `amb_city_bed` by altitude) |
| amb_arcade_bleeps | Turtle Lair | quiet arcade-cabinet attract-mode bleeps + CRT whine near the lair's neon (currently `amb_neon_hum`) |
