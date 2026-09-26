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

## Controls (rebind in Settings → Controls)
| key | |
|---|---|
| Mouse | look |
| W A S D | move |
| Space | jump |
| Ctrl (or C) | crouch |
| Shift | boost |
| Left Alt | dodge |
| Q / E (or Mouse 4 / 5) | left / right tether |
| R or wheel | reel |
| LMB / RMB | use |
| RMB with a gun | aim |
| H | gun alt-fire |
| Enter or T | ring |
| V (hold) | voice |
| 1–4 | quick cast |
| F | oath |
| Z (hold) | charge |
| G (tap / hold) | power |
| Mouse 3 | transform |
| E in a seat | eject |
| X | dismiss |
| F5 | view |
| B | shoulder |
| Tab | scores |
| Esc | menu |
| F3 · F11 (or Alt+Enter) | performance · fullscreen |

That's all the help there is. Everything else is for you to find out.

## Settings
**Settings** (title menu, or Esc in a match). Q / E switch tabs.
- **Gameplay** — default view (first / third person), shoulder, aim-down-sights hold / toggle and zoom speed, voice casting.
- **Controls** — mouse sensitivity, ADS sensitivity, invert Y, every key. Taking a key another action uses swaps the two.
- **Video** — field of view, fullscreen, vsync, anti-aliasing, latency options (next launch).
- **Audio** — master / music / world / ambience / hit feedback volume, music style, reduced dynamic range, mono.
- **Comfort** — the comfort preset and each of its parts.
- **HUD** — player outlines (thickness, colour-blind patterns), enemy compass, damage % number, shout words (on / mine only / off), comic words, summon preview.

## Comfort
Settings → Comfort → **Comfort preset** turns off shake, FOV kicks and flashes, adds a vignette, and
makes view switches and ADS zoom instant cuts; each part can be set on its own. Third person (F5, or
Settings → Gameplay → Default view) is a full way to play and often easier on the stomach too.
Recommended for anyone who gets motion sick.

## For the host
- Match rules (every knob) and presets live on the host setup page: Classic, Party, Chaos, Movement Gods.
- Tuning files (`assets/tuning/*.ron`) hot-reload while the game runs.
- Phrases the ring didn't understand are logged to `logs/unresolved.txt` — send it back to grow the
  vocabulary.
