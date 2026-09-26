"""Which slices of Wikidata we harvest, and how deep into the long tail we go.

A *family* is a coarse bucket (pokemon, kaiju, fiction_character, weapon, taxon, ...) that the lexicon
can use as a prior for body plan / Form. Families are matched in the order listed: an entity reached from
several roots gets the first family. Nothing here says anything about behaviour -- only what things ARE.

Root spec: (QID, label-for-humans, inst_min_sitelinks or None, type_min_sitelinks or None)
  inst: items that are `wdt:P31/wdt:P279*` of the root (individuals: Eeyore, Big Ben, USS Enterprise)
  type: items that are `wdt:P279+` of the root (kinds: longsword, ocarina, burrito, corgi-as-class)
None disables that mode for the root.
"""

FAMILIES: list[tuple[str, list[tuple[str, str, int | None, int | None]]]] = [
    ("pokemon", [
        ("Q3966183", "Pokémon species", 1, None),
        ("Q1608017", "Digimon", 1, None),
    ]),
    ("kaiju", [
        ("Q1065444", "kaiju", 1, 2),
    ]),
    ("mecha", [
        ("Q858784", "mecha", 1, 2),
    ]),
    ("robot", [
        ("Q11012", "robot", 2, 3),
    ]),
    ("myth_creature", [
        ("Q2239243", "mythical creature", 1, 2),
        ("Q772636", "cryptid", 1, 2),
        ("Q276453", "monster", 1, 2),
        ("Q7559", "dragon", 1, 2),
    ]),
    ("fiction_character", [
        ("Q95074", "character (fictional)", 1, 3),
    ]),
    ("fiction_other", [
        # fictional objects, weapons, vehicles, spacecraft, species, places ...
        ("Q14897293", "fictional entity", 1, 3),
        ("Q15831596", "class of fictional entities", 1, None),   # lightsaber, Keyblade-style kinds
        ("Q124980912", "video game weapon", 1, 1),
    ]),
    ("myth_other", [
        ("Q24334685", "mythical entity", 2, 3),
        ("Q20203727", "mythical object", 1, 2),
        ("Q178885", "deity", 3, 3),
    ]),
    ("breed", [
        ("Q38829", "breed", 1, 2),
    ]),
    ("weapon", [
        ("Q728", "weapon", 3, 3),
        ("Q15142894", "weapon model", 2, None),
        ("Q22704163", "firearm model", 3, None),
    ]),
    ("spacecraft", [
        ("Q40218", "spacecraft", 3, 3),
        ("Q117384800", "spacecraft model", 2, None),
        ("Q697175", "launch vehicle", 3, 3),
        ("Q109542585", "rocket family", 2, None),
    ]),
    ("aircraft", [
        ("Q11436", "aircraft", 5, 3),
        ("Q15056995", "aircraft model", 3, None),
    ]),
    ("ship", [
        ("Q11446", "ship", 5, 3),
        ("Q559026", "ship class", 3, None),
    ]),
    ("vehicle", [
        ("Q42889", "vehicle", 5, 3),
        ("Q29048322", "vehicle model", 3, None),
        ("Q3231690", "car model", 3, None),
        ("Q19832486", "locomotive class", 3, None),
    ]),
    ("structure", [
        ("Q179700", "statue", 15, 5),
        ("Q4989906", "monument", 25, 5),
        ("Q811979", "architectural structure", 25, 5),
        ("Q570116", "tourist attraction", 30, None),
        ("Q4895393", "landmark", 25, None),
    ]),
    ("instrument", [
        ("Q34379", "musical instrument", 3, 2),
    ]),
    ("food", [
        ("Q2095", "food", 5, 3),
        ("Q746549", "dish", 3, 3),
        ("Q40050", "drink", 5, 3),
    ]),
    ("toy", [
        ("Q11422", "toy", 2, 2),
        ("Q8076", "video game console", 3, 3),
        ("Q13698", "puzzle", 3, 3),                 # Rubik's Cube
        ("Q1187635", "digital pet", 2, 2),          # Tamagotchi
    ]),
    ("sports_equipment", [
        ("Q768186", "sports equipment", 3, 2),
    ]),
    ("appliance", [
        ("Q212920", "home appliance", 3, 2),
        ("Q14745", "furniture", 5, 3),
    ]),
    ("tool", [
        ("Q39546", "physical tool", None, 3),     # individuals under `tool` are mostly reservoirs etc.
    ]),
    ("product", [
        ("Q10929058", "product model", 10, None),   # well-known manufactured things (Game Boy, iPhone)
    ]),
    ("landform", [
        ("Q8502", "mountain", 60, None),            # Mount Rushmore, Everest, Fuji
        ("Q8072", "volcano", 30, None),
    ]),
    ("franchise", [
        # a player saying "Voltron" / "Mazinger Z" / "Power Rangers" means the robot/hero; keep the name
        ("Q196600", "media franchise", 10, None),
    ]),
    ("taxon", [
        # common-named organism groups (dog, whale, shark, crab ...); taxa proper are harvested specially
        ("Q55983715", "organisms known by a particular common name", 3, None),
    ]),
]

# Extra candidate queries: (family, kind, SPARQL returning ?i ?sl). Items with a "from narrative
# universe" statement are fictional even when their P31 is missing or odd (Batmobile has only P279).
EXTRA_QUERIES = [
    ("fiction_other", "instance",
     "SELECT ?i ?sl WHERE { ?i wdt:P1080 ?u . ?i wikibase:sitelinks ?sl "
     "FILTER NOT EXISTS { ?i wdt:P577 ?d } FILTER NOT EXISTS { ?i wdt:P31 wd:Q5 } }"),
]

# Taxa: (English common name P1843 AND >= TAXON_CN_MIN sitelinks) OR >= TAXON_MIN sitelinks.
TAXON_CN_MIN = 3
TAXON_MIN = 20
TAXON_FOSSIL_MIN = 8   # P31 fossil taxon (dinosaurs, mammoths ...) rarely have P1843 common names

# Roots whose subclass trees mark an entity as fictional/legendary.
FICTIONAL_ROOTS = ["Q14897293", "Q95074", "Q24334685", "Q2239243", "Q772636", "Q20203727", "Q1065444"]

# Classes never used as harvest classes even if some bad P279 edge puts them in a tree.
CLASS_BLOCKLIST = {
    "Q5",          # human
    "Q16521",      # taxon (handled separately)
    "Q13442814",   # scholarly article
    "Q4167836",    # Wikimedia category
    "Q4167410",    # disambiguation page
    "Q13406463",   # list article
    "Q11266439",   # template
    "Q7725634",    # literary work
    "Q11424",      # film
    "Q5398426",    # television series
    "Q7889",       # video game
    "Q482994",     # album
    "Q134556",     # single
    "Q43229",      # organization
    "Q4830453",    # business
    "Q515",        # city
    "Q486972",     # human settlement
}

# Real-world (non-fiction) harvest trees are pruned of any class that is also an event/process/
# organisation/settlement/administrative area: bad P279 edges otherwise pull in universities, papal
# journeys and municipalities as "architectural structures". Entities of real families whose P31 falls
# in these trees are dropped too (Heligoland is a municipality, not a landmark).
# (occurrence/process/activity trees are unusable: bad edges put `food` under them.)
EXCLUDE_ROOTS = ["Q486972", "Q56061", "Q43229", "Q1656682"]  # settlement, admin area, organisation, planned event
PRUNED_FAMILIES = {"structure", "tool", "appliance", "landform", "sports_equipment", "toy"}
EXCLUDE_KEEP_ROOTS = ["Q23413"]                               # castle (sits under admin area via lordship)
# A real-world instance whose P31 hits an excluded class is still kept if it is also a built thing.
BUILT_ROOTS = ["Q41176", "Q12518", "Q12280", "Q179700", "Q23413", "Q4989906"]
FICTION_FAMILIES = {"pokemon", "kaiju", "mecha", "myth_creature", "fiction_character", "fiction_other",
                    "myth_other", "franchise"}

# Ancestor classes too generic to be worth listing in a class chain (matched on English label).
GENERIC_LABELS = {
    "entity", "object", "physical object", "concrete object", "artificial physical object",
    "artificial object", "abstract object", "abstract entity", "class", "concept", "item", "matter",
    "physical entity", "spatio-temporal entity", "occurrent", "continuant", "independent continuant",
    "material entity", "occurrence", "phenomenon", "system", "whole", "part", "product", "goods",
    "artificial entity", "individual", "thing", "particular", "type", "collective entity", "group",
    "role", "physical system", "observable entity", "temporal entity", "spatial entity",
    "three-dimensional object", "geometric shape", "mental representation", "work", "creative work",
    "manufactured good", "artifact", "artefact", "idea", "information", "representation", "result",
    "resource", "means", "subject", "unit", "structure", "construction", "component", "set",
    "multi-organism group", "second-order class", "metaclass", "first-order class", "variable-order class",
    "fixed-order metaclass", "class of instances", "Wikidata metaclass", "concrete entity",
}

# Taxon ranks always kept in a taxon's class chain (genus, family, order, class, phylum, kingdom);
# other ancestor clades (Dinosauria, Serpentes, Theropoda ...) are kept if well known.
KEY_RANKS = {"Q34740", "Q35409", "Q36602", "Q37517", "Q38348", "Q36732"}
CLADE_MIN_SITELINKS = 80
TAXON_CHAIN_MAX = 10

DINOSAURIA = "Q430"
AVES = "Q5113"
