import re, sys
from dsl import MECH_ORDER, FEAT_ORDER, FORMS, ELEMENTS, TEMPS, BLOCK
from names_data import DATA

# Generic canonical words get a franchise qualifier; generic aliases / quotes are dropped.
RENAME = dict(l.split("=") for l in """link=link zelda|hammer=mario hammer|toad=mario toad|recall=|fi=fi zelda|demise=demise zelda
|stal=stalfos|talus=stone talus|ball and chain=zelda ball and chain|remote bomb=sheikah remote bomb|stasis=zelda stasis
|beetle=zelda beetle|spinner=|amp=mario amp|pokey=mario pokey|urchin=mario urchin|fuzzy=mario fuzzy|spiny=mario spiny
|blooper=mario blooper|bloober=|flagpole=mario flagpole|donkey kong bananza pauline=|halberd=halberd battleship
|ice beam=samus ice beam|isabelle=isabelle animal crossing|villager=animal crossing villager|roy=roy fire emblem
|robin=robin fire emblem|sandbag=smash sandbag|bumper=smash bumper|earthquake=earthquake move|surf=surf move
|thunderbolt=thunderbolt move|flamethrower=flamethrower move|breeze=minecraft breeze|phantom=minecraft phantom
|drowned=minecraft drowned|blaze=minecraft blaze|vex=minecraft vex|axolotl=minecraft axolotl|armadillo=minecraft armadillo
|beacon=minecraft beacon|bedrock=minecraft bedrock|obsidian=minecraft obsidian|piston=minecraft piston|cobweb=minecraft cobweb
|fishing rod=minecraft fishing rod|spyglass=|brush minecraft=|cake minecraft=|the device=fortnite device|drift=fortnite drift
|raven=fortnite raven|elite=halo elite|brute=halo brute|jackal=halo jackal|flood=the flood|monitor=343 guilty spark
|sentinel=halo sentinel|carbine=covenant carbine|mantis=halo mantis|elephant=halo elephant|scarab=covenant scarab
|assault rifle=ma5 assault rifle|battle rifle=br55 battle rifle|imp=doom imp|revenant=doom revenant|marauder=doom marauder
|ballista=doom ballista|airboat=half-life airboat|engineer=tf2 engineer|dispenser=tf2 dispenser|frying pan=tf2 frying pan
|humanity=|bonfire=dark souls bonfire|patches=patches souls|fire giant=elden ring fire giant|let me solo her tarnished=
|amygdala=bloodborne amygdala|spectral steed whistle=|ribbon=|elixir=ff elixir|odin=odin ff|garuda=garuda ff|shinra=
|regalia=regalia ff|axel=axel kh|nobody=kh nobody|frog chrono=frog chrono trigger|freya=freya gow|baldur=baldur gow
|fenrir=fenrir gow|hydra=hydra gow|colossus of rhodes=gow colossus of rhodes|cronos=cronos gow|the boss=the boss mgs
|vamp=vamp mgs|gekko=gekko mgs|exclamation mark=metal gear alert|wraith=wraith apex|bloodhound=bloodhound apex
|lifeline=lifeline apex|caustic=caustic apex|mirage=mirage apex|crypto=crypto apex|seer=seer apex|catalyst=catalyst apex
|alter=alter apex|conduit=conduit apex|horizon=horizon apex|valkyrie=valkyrie apex|ballistic=ballistic apex
|bangalore=bangalore apex|newcastle=newcastle apex|havoc=havoc rifle|wingman=wingman pistol|peacekeeper=peacekeeper shotgun
|heirloom=apex heirloom|zipline=apex zipline|tracer=tracer overwatch|mercy=mercy overwatch|ana=ana overwatch
|bastion=bastion overwatch|reaper=reaper overwatch|echo=echo overwatch|wrecking ball=wrecking ball overwatch
|sigma=sigma overwatch|venture=venture overwatch|juno=juno overwatch|hazard=hazard overwatch|raze=raze valorant
|sage=sage valorant|brimstone=brimstone valorant|omen=omen valorant|viper=viper valorant|killjoy=killjoy valorant
|cypher=cypher valorant|breach=breach valorant|astra=astra valorant|chamber=chamber valorant|neon=neon valorant
|fade=fade valorant|harbor=harbor valorant|deadlock=deadlock valorant|iso=iso valorant|clove=clove valorant
|waylay=waylay valorant|vandal=vandal rifle|operator=operator valorant|sheriff=|classic=|jinx=jinx league|vi=vi league
|lux=lux league|darius=darius league|elder dragon=elder dragon league|viktor=viktor league|gangplank=gangplank league
|graves=graves league|juggernaut=cod juggernaut|hellhound=cod hellhound|price=captain price|franklin=franklin gta
|trevor=trevor gta|buzzard=buzzard gta|insurgent=insurgent gta|dodo=dodo gta|blimp=gta blimp|bigfoot=|ripper=
|regenerator=re regenerator|merchant=re4 merchant|typewriter=|dante=dante dmc|rebellion=rebellion sword|nero=nero dmc
|red queen=red queen sword|ryu=ryu street fighter|ken=ken street fighter|guile=guile street fighter
|akuma=akuma street fighter|scorpion=scorpion mk|jax=jax mk|kuma=kuma tekken|tails=tails sonic|knuckles=knuckles sonic
|the knight=little ghost|grimm=grimm hollow knight|lace=lace silksong|shaw=|shade=hollow knight shade
|the silent=slay the spire silent|the defect=slay the spire defect|the watcher=slay the spire watcher|the heart=corrupt heart
|cultist spire=|isaac=binding of isaac|papyrus=papyrus undertale|lamb=cult of the lamb|stray cat=
|the destroyer=the destroyer terraria|queen bee=queen bee terraria|dead body=|shapeshifter=among us shapeshifter
|scientist=|schedule i=|bracken=bracken lethal company|thumper=thumper lethal company|jester=jester lethal company
|nutcracker=nutcracker lethal company|stop sign=|megabonk=|dispatch=|content warning=|chained together=|oof=
|brookhaven=|grow a garden=|steal a brainrot=|centipede=centipede arcade|asteroid=|ratchet=ratchet lombax|clank=clank robot
|sawtooth=sawtooth horizon|007=007 first light|lune=lune expedition 33|normandy=ss normandy|mako=mako mass effect
|lancer=lancer gears|locust=locust horde|battlecruiser=starcraft battlecruiser|zealot=protoss zealot|archon=protoss archon
|nuclear launch detected=starcraft nuke|thorn=destiny thorn|ace of spades=ace of spades destiny|sparrow=destiny sparrow
|golden gun=golden gun destiny|cabal=destiny cabal|ghost destiny=|lightning mcqueen=|sweet tooth=sweet tooth twisted metal
|golf club=|wii bowling ball=wii bowling|kratos shield=|fox mccloud=fox mccloud|the ironclad=slay the spire ironclad
|pod 042=pod 042|dominus=roblox dominus|hearthstone=|joel=joel tlou|ellie=ellie tlou|altair=altair assassin|sans=sans undertale|melina=melina elden ring
|godfrey=godfrey elden ring|ludwig=ludwig bloodborne|emil=emil nier|reyna=reyna valorant|skye=skye valorant
|morgana=morgana league|sion=sion league|katarina=katarina league|caitlyn=caitlyn league|cj=cj gta|goro=goro mk
|baraka=baraka mk|madeline=madeline celeste|ori=ori spirit|yasuke=yasuke assassin's creed|bloater=bloater tlou
|gustave=gustave expedition 33|oryx=oryx destiny|pyro=pyro tf2|shambler=quake shambler|oddball=halo oddball
|mongoose=halo mongoose|arbiter=halo arbiter|midas=midas fortnite|ravager=minecraft ravager|steve=steve minecraft
|rollout=rollout move|inkling=inkling splatoon|inkjet=inkjet splatoon|pit=pit kid icarus|ike=ike fire emblem
|ness=ness earthbound|lucas=lucas earthbound|emmi=emmi metroid|marx=marx kirby|pauline=pauline mario|sheik=sheik zelda
|tingle=tingle zelda|roshan=roshan dota|winston=winston overwatch|mei=mei overwatch""".replace("\n", "").split("|"))
DROP_ALIAS = set("""hey listen|praise the sun|do a barrel roll|whatcha buyin|cheers love|he's on fire|okay|hmm villager|caw caw
|demacia|one winged angel|boy|the ball|pump|peak|chaos|the moon|spike|tick|op|fans|panda|psycho|moped|thruster|heavy
|skateboard|bowling ball|shopping cart|football fc|the prince|daisy|peach|fox|ocarina|firestorm|alexander|cloud|snake
|omega|pod|epoch|leon|diana|lucia|james bond|first light|noob|employee|taser|the traveler|sovereign|zerg|mrglglgl
|one winged angel|yoga fire|chakram|falchion|monado|materia|luma|koopa|koopas|heavy|op|dizzy|mosh|wingman valorant
|spike|gold ring|tabs|rocket jump|okay|the traveler""".replace("\n", "").split("|"))


def parse_hint(h, name):
    toks = h.split()
    if not toks or toks[0] not in FORMS:
        raise ValueError(f"{name}: first token must be a form: {h!r}")
    form = toks[0]
    mech, feat, wields, el, temp = [], [], [], None, None
    for t in toks[1:]:
        if t.startswith("@"):
            assert t[1:] in ELEMENTS, (name, t); el = t[1:]
        elif t.startswith("!"):
            assert t[1:] in TEMPS, (name, t); temp = t[1:]
        elif t in MECH_ORDER:
            mech.append(t)
        elif t in FEAT_ORDER:
            feat.append(t)
        elif t in ELEMENTS:
            el = t
        elif t in TEMPS:
            temp = t
        elif t in FORMS:
            wields.append(t)
        else:
            raise ValueError(f"{name}: unknown token {t!r}")
    mech = sorted(set(mech), key=MECH_ORDER.index)
    s = form
    if mech:
        s += " + " + ", ".join(mech)
    extra = []
    if el:
        extra.append(el)
    if wields:
        extra.append("wields " + "/".join(wields))
    if feat:
        extra.append("features " + "/".join(feat))
    if temp:
        extra.append(temp)
    if extra:
        s += " (" + "; ".join(extra) + ")"
    return s


def main(ron_path, out_path):
    ron = open(ron_path, encoding="utf-8").read()
    taken = set(re.findall(r'"([^"]+)"', "\n".join(l for l in ron.splitlines() if "names:" in l)))
    taken |= BLOCK
    import glob, os
    for p in glob.glob(os.path.join(os.path.dirname(ron_path), "*.ron")):
        if os.path.abspath(p) == os.path.abspath(ron_path):
            continue
        t = open(p, encoding="utf-8").read()
        for m in re.finditer(r'names:\s*\[([^\]]*)\]', t):
            taken |= set(re.findall(r'"([^"]+)"', m.group(1)))
    out, errors, seen = [], [], set()
    section = ""
    for line in DATA.strip().splitlines():
        line = line.strip()
        if not line:
            continue
        if line.startswith("##"):
            section = line[2:].strip()
            continue
        parts = [p.strip() for p in line.split("|")]
        if len(parts) != 3:
            errors.append(f"bad line: {line}"); continue
        name, aliases, hint = parts
        name = name.lower()
        if name in RENAME:
            name = RENAME[name]
            if not name:
                continue
        try:
            h = parse_hint(hint, name)
        except (ValueError, AssertionError) as e:
            errors.append(str(e)); continue
        if name in taken or name in seen:
            continue
        al = []
        for a in aliases.split(";"):
            a = a.strip().lower()
            if a and a not in DROP_ALIAS and a not in taken and a not in seen and a != name and a not in al:
                al.append(a)
        seen.add(name)
        seen.update(al)
        out.append(f"{name} | {'; '.join(al)} | | hint: {h} [{section}]".replace(' |  | ', ' | | '))
    if errors:
        print("\n".join(errors))
        sys.exit(1)
    with open(out_path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("# Brightest Day - Tier-A video game names (characters, creatures, items, weapons, vehicles).\n")
        fh.write("# name | aliases (;) | Wikidata QID (blank = unresolved) | hint: Form + Mechanics (element; wields; features; temperament) [franchise]\n")
        fh.write("\n".join(out) + "\n")
    print(len(out), "names")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
