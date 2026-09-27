"""Military + tabletop (D&D) single words that the icon packs cannot claim.

Domain icon packs (assets/icons/military.ron, assets/icons/tabletop.ron) yield common single words to the
lexicon (bd_lexicon `shadow_domain_aliases`), so words like "bunker", "wish" or "haste" are anchored here
instead. Same format as anchors.WORD_ANCHORS (merged by build.py after everyday.EVERYDAY):
  f=Form, m=[Mechanic...], e=Element, s=scale m, w=mass kg, count=n, t=Temperament,
  g=[graft mechanics], part=True (cast alone = graft), ft=[Feature...]
Behaviour only from the closed sets in bd_core::mechanics; names only, never rules text.
"""

ANCHORS = {
    # ------------------------------------------------------------------ military
    "bunker": dict(f="Dome", m=["Barrier", "Heavy"], e="Earth", s=4.0, ft=["Windows"]),
    "bunkers": dict(f="Dome", m=["Barrier", "Heavy"], e="Earth", s=4.0, ft=["Windows"]),
    "trench": dict(f="Wall", m=["Barrier"], e="Earth", s=2.0),
    "trenches": dict(f="Wall", m=["Barrier"], e="Earth", s=2.0),
    "rpg": dict(f="Cannon", m=["Shoots", "Explodes"], e="Fire", s=1.2),
    "torpedo": dict(f="Rocket", m=["Homing", "Explodes", "Fast"], e="Water", s=2.0),
    # ------------------------------------------------------------------ tabletop spells (single words)
    # the one spell that does anything: a falling star
    "wish": dict(f="Boulder", m=["DropFromSky", "Explodes", "Heavy"], e="Light", s=3.0),
    "hex": dict(f="Blob", m=["Homing", "Flies", "Sticky"], e="Shadow", s=0.5, g=["Homing"]),
    "grease": dict(f="Platform", m=["Sticky"], e="Earth", s=3.0, g=["Sticky"]),
    "haste": dict(f="Blob", m=["Fast", "Flies"], e="Wind", s=0.8, t="Playful", g=["Fast"]),
    "teleport": dict(f="Platform", m=["Portal"], e="Psychic", g=["Portal"]),
    "teleportation": dict(f="Platform", m=["Portal"], e="Psychic", g=["Portal"]),
}

# Graft verbs ("reduce it", "make it reduce"): anchors.VERBS format.
VERBS = {
    "reduce": ["Shrink"],
}
