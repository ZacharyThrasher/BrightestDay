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
| amb_casino_walla | High Roller | casino-floor bed: indistinct crowd murmur (no words), distant slot jingles, chip clatter, a far cheer now and then — loopable, carpet-dead (currently `amb_interior_tone` + `amb_ac_hum`) |
| amb_slot_jingle | High Roller | short cheerful slot-machine attract melodies / payout bells from somewhere on the floor (sparkle, Surface zone) |
| amb_chip_clatter | High Roller | stacks of clay chips being shuffled / dropped on felt (sparkle; `assemble_click` stands in) |
| amb_card_shuffle | High Roller | riffle shuffle + card snaps from the tables (sparkle; `transform_panel_click` stands in) |
| slot_lever | High Roller slot machines | big mechanical lever yank: ratchet + spring return (`hatch_close` stands in) |
| slot_clunk | High Roller | lever hit with no stake: a dull locked clunk (`transform_panel_click` pitched down stands in) |
| slot_reel_loop | High Roller | reels whirring (loop, stops abruptly) + per-reel stop `thunk` (`rotor_loop` pitched up / `assemble_click` stand in) |
| slot_win | High Roller | three-of-a-kind payout: bright bell arpeggio + a few coins (`oath_complete` stands in) |
| slot_jackpot | High Roller | JACKPOT: big bell run + cascading coins + siren-ish sweep, ~2.5 s, celebratory not harsh (`legendary_sting` + `charge_full` stand in) |
| slot_bust_trombone | High Roller | the bust: comic "wah-wah-wah-waaah" sad trombone, ~1.6 s (a sound cue, not the bd_music `Defeat` stinger — that ends the track); `sputter` + `shout_fizzle` stand in |
| pie_splat | High Roller bust | wet cream-pie splat on your face (`land_heavy` pitched up stands in) |
| confetti_pop | High Roller bust | party popper + paper flutter (`shout_release` stands in) |
| spring_boing | High Roller bust | cartoon spring "boing" for the little hop (`air_jump` pitched down stands in) |
| roulette_spin | High Roller house spin | wheel rumble + ball rattle round the wooden track, decelerating over `house_sweep` s (`rotor_loop` pitched down stands in) |
| roulette_ball_drop | High Roller house spin | the ball clacking over the frets and dropping into a pocket (`assemble_lock` stands in) |
| house_bonus | High Roller house spin | small chip "cha-ching" when you step into the lit pocket (`charge_full` pitched up stands in) |
| amb_furin | Sakura Keep | glass wind-chime tinkles under eaves (sparkle, Wall/Leaves zone) |
| amb_temple_bell | Sakura Keep | distant temple bell: one deep bonnnng with a long beat-y tail, rare (sparkle, Sky zone) |
| amb_fountain | Sakura Keep | small trickling water basin / koi pond (loop near the shrine; needs a map-placed emitter) |
