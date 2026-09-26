# Brightest Day — how to play with friends

A 1–8 player free-for-all. You move like Quake, swing like Attack on Titan, and fight by **saying or
typing anything** — your ring builds it out of hard light. Get knocked far enough and you're out.

## Start
Unzip anywhere and run `brightest_day.exe`. No install. First launch compiles shaders for a few seconds.

- **Tutorial** — learn the movement (the ghost shows you the line). Beat gold on the Peak Run.
- **Practice** — fight bots on any map with your rules.
- **Host** — pick a map and rules, then **Open lobby**. Share the **lobby code** (big text) with friends.
  Friends on the same network can use the **LAN address** shown instead (faster).
- **Join** — paste a lobby code, a LAN address like `192.168.1.20:27960`, or a tunnel URL.

Online hosting uses a free Cloudflare quick tunnel (`cloudflared.exe`, bundled). No accounts, no port
forwarding. If it can't start, LAN play still works.

## Controls (rebindable in Settings)
| | |
|---|---|
| Mouse | look |
| WASD / Space / Ctrl | move / jump (hold to keep hopping) / crouch-slide |
| Shift | gas boost |
| Q / E (or Mouse 4 / 5) | left / right tether — hold to stay attached, release to fling |
| Wheel / R | reel in |
| LMB / RMB | use what you're holding (empty hand: ring blast / shove) |
| Enter or T | open the ring — type anything, Enter to make it |
| V (hold) | say it instead of typing it (if built with voice) |
| 1–4 | your four saved phrases |
| G | power (tap / hold) · Mouse 3: transform · E: eject |
| F | recite your oath to recharge willpower |
| Z (hold) | charge |
| X | dismiss your constructs · Tab: scores · F3: performance |

That's all the help there is. Everything else is for you to find out.

## Comfort
Settings → **Comfort** preset turns off shake, FOV kicks and flashes and adds a vignette; there's also a
third-person camera. Recommended for anyone who gets motion sick.

## For the host
- Match rules (every knob) and presets live on the host setup page: Classic, Party, Chaos, Movement Gods.
- Tuning files (`assets/tuning/*.ron`) hot-reload while the game runs.
- Phrases the ring didn't understand are logged to `logs/unresolved.txt` — send it back to grow the
  vocabulary.
