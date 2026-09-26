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
| legendary_sting | freedom bundle | short golden shimmer sting (hidden mechanic — no "reward" voice) |
| sputter | freedom bundle | flickering, underpowered cast |
| ads_in / ads_out | camera/ADS | subtle cloth/weapon raise |
| view_toggle | camera | very subtle UI whoosh |
