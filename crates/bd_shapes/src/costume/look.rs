//! Character looks (assets/costumes/characters.ron, compiled in): data model, parsing, alias
//! lookup and variant merging. Building the voxels from a [`Look`] is `style.rs`.
//!
//! A look is a set of picks from the style library (hair style, eyewear, helmet, outfit cut, …) with
//! exact colours. Variants are *patches*: any field they set replaces the base look's.
//! Colours are strings: `"f4f4f8"` / `"#f4f4f8"` or a few names (`"black"`, `"white"`, `"skin"`…).

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Deserializer};

use super::paint::Rgb;

/// sRGB colour parsed from a string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Col(pub Rgb);

impl Col {
    pub fn rgb(self) -> Rgb {
        self.0
    }
    pub fn scale(self, k: f32) -> Rgb {
        self.0.map(|c| (c as f32 * k).round().clamp(0.0, 255.0) as u8)
    }
}

/// Parse `"rrggbb"`, `"#rrggbb"`, `"rgb"` or a colour name.
pub fn parse_col(s: &str) -> Option<Rgb> {
    let t = s.trim().trim_start_matches('#').to_lowercase();
    let named = match t.as_str() {
        "black" => Some([22, 22, 26]),
        "white" => Some([240, 240, 244]),
        "skin" | "fair" => Some([244, 204, 170]),
        "tan" => Some([214, 164, 120]),
        "brown_skin" | "brown skin" => Some([160, 108, 72]),
        "dark_skin" | "dark skin" => Some([104, 68, 48]),
        "red" => Some([210, 30, 36]),
        "blue" => Some([30, 70, 200]),
        "navy" => Some([24, 30, 64]),
        "green" => Some([40, 150, 60]),
        "yellow" => Some([250, 210, 40]),
        "gold" => Some([230, 180, 50]),
        "silver" => Some([190, 196, 206]),
        "grey" | "gray" => Some([120, 124, 132]),
        "orange" => Some([245, 120, 20]),
        "purple" => Some([120, 50, 170]),
        "pink" => Some([245, 140, 180]),
        "brown" => Some([110, 70, 40]),
        _ => None,
    };
    if named.is_some() {
        return named;
    }
    let hex = |i: usize, n: usize| u8::from_str_radix(&t[i..i + n], 16).ok();
    match t.len() {
        6 if t.chars().all(|c| c.is_ascii_hexdigit()) => Some([hex(0, 2)?, hex(2, 2)?, hex(4, 2)?]),
        3 if t.chars().all(|c| c.is_ascii_hexdigit()) => Some([hex(0, 1)? * 17, hex(1, 1)? * 17, hex(2, 1)? * 17]),
        _ => None,
    }
}

impl<'de> Deserialize<'de> for Col {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        parse_col(&s).map(Col).ok_or_else(|| serde::de::Error::custom(format!("bad colour {s:?}")))
    }
}

// ------------------------------------------------------------------------------------ style picks

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum HairStyle {
    Bald,
    Buzz,
    Short,
    Messy,
    /// Tall upward spikes (Gojo).
    SpikyUp,
    /// Spikes all around (Naruto).
    SpikyWild,
    /// Big fanned spikes + forehead bangs (Goku).
    Saiyan,
    /// One tall flame (Vegeta).
    Flame,
    /// Spikes swept back (Sasuke).
    SpikyBack,
    Long,
    Bob,
    Ponytail,
    TwinTails,
    Bun,
    Afro,
    Mohawk,
    Slicked,
    Swept,
    Shaggy,
    Topknot,
    /// Short with a single curl/antenna (Luffy-ish messy + ahoge).
    Tufty,
    /// Two buns over the ears (Leia / Chun-Li).
    SideBuns,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Fringe {
    Full,
    Parted,
    Spiky,
    Side,
    Open,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct Hair {
    pub style: HairStyle,
    pub color: Col,
    #[serde(default)]
    pub tips: Option<Col>,
    #[serde(default)]
    pub fringe: Option<Fringe>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum EyeStyle {
    Normal,
    Big,
    Narrow,
    Glow,
    Closed,
    Angry,
    Dots,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct Eyes {
    pub color: Col,
    #[serde(default = "eye_normal")]
    pub style: EyeStyle,
}
fn eye_normal() -> EyeStyle {
    EyeStyle::Normal
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Side {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub enum Facial {
    Beard(Col),
    LongBeard(Col),
    Goatee(Col),
    Moustache(Col),
    BigMoustache(Col),
    Stubble(Col),
    Scar(Col, Side),
    /// Vertical scar through an eye (Zoro / Kratos style).
    EyeScar(Col, Side),
    Whiskers(Col),
    /// Cheek marks (two short stripes per cheek, Inuyasha/Tanjiro-ish).
    CheekMarks(Col),
    /// Forehead mark (a small symbol).
    ForeheadMark(Col),
    /// Face tattoo stripe (one side, forehead to chin).
    Stripe(Col, Side),
    Blush(Col),
    Fangs,
    Lipstick(Col),
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub enum Eyewear {
    No,
    Blindfold(Col),
    RoundShades(Col),
    Shades(Col),
    Glasses(Col),
    Visor(Col),
    Goggles(Col, Col),
    Eyepatch(Col, Side),
    Bandage(Col),
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub enum Mask {
    No,
    /// Full head mask with big white lenses and web lines.
    Spider(Col, Col),
    /// Full head mask (base, eye patch colour, lens colour).
    Full(Col, Col, Col),
    /// Cowl covering the head except the mouth/chin; `true` = pointed ears.
    Cowl(Col, bool),
    /// Lower-face mask.
    Ninja(Col),
    /// Eye band tied at the back (TMNT).
    Bandana(Col),
    Oni(Col),
    Hockey(Col),
    Skull(Col),
    /// Wolverine-style cowl with swept flares.
    Flared(Col, Col),
    /// Domino eye mask (Robin, Zorro).
    Domino(Col),
    /// Rebreather / respirator on the lower face.
    Respirator(Col),
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub enum Helmet {
    No,
    Vader(Col),
    Trooper(Col, Col),
    /// T-visor helmet (base, visor).
    Mando(Col, Col),
    /// Boba Fett: base, visor, trim.
    Boba(Col, Col, Col),
    /// Master Chief (base, visor).
    Chief(Col, Col),
    Doom(Col, Col),
    /// Iron Man: helmet, faceplate, eye glow.
    IronMan(Col, Col, Col),
    Knight(Col, Option<Col>),
    Space(Col, Col),
    Robot(Col, Col),
    Samus(Col, Col),
    /// Horned helmet (Loki / Viking): base, horn colour.
    Horned(Col, Col),
    /// Spartan crest helmet (Leonidas / Kratos-ish): base, crest.
    Crested(Col, Col),
    /// Bucket helmet with a vertical slit (Kylo / Mandalorian-ish masks).
    Slit(Col, Col),
    /// Daft-punk-ish smooth helmet with a visor band.
    Smooth(Col, Col),
    /// Chainsaw head (skin/hood colour, blade colour).
    Chainsaw(Col, Col),
    /// Autobot head: helmet, faceplate, eye glow.
    Prime(Col, Col, Col),
    /// Mega Man helmet: base, stripe/ear colour.
    Mega(Col, Col),
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub enum Hat {
    No,
    /// Cap with an optional letter badge colour (Mario "M").
    Cap(Col, Option<Col>),
    /// Long drooping elf cap (Link).
    Pointy(Col),
    Straw(Col, Col),
    Wizard(Col, Col),
    Cowboy(Col),
    Fedora(Col, Col),
    TopHat(Col, Col),
    Crown(Col),
    Beanie(Col),
    /// Headband with an optional metal plate (Naruto forehead protector).
    Headband(Col, Option<Col>),
    /// Bandana tied at the back with trailing ends.
    Bandana(Col),
    Horns(Col),
    Halo(Col),
    CatEars(Col),
    Hood(Col),
    Tiara(Col),
    /// Winged helmet cap (Thor classic).
    Winged(Col, Col),
    /// Upward horns on a band (Loki).
    LokiHorns(Col),
    Beret(Col),
    Bowler(Col),
    /// Paper/bucket hat.
    Bucket(Col),
    Headphones(Col),
    /// Hachimaki-style tied headband without plate (Ryu).
    Tied(Col),
    /// Pirate tricorn (hat, trim).
    Tricorn(Col, Col),
    /// Big side ears (elf / ogre).
    Ears(Col),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Cut {
    Bare,
    Tee,
    Tank,
    LongSleeve,
    HighCollar,
    Gi,
    Robe,
    Plate,
    Power,
    Suit,
    Jumpsuit,
    Hoodie,
    Coat,
    Tunic,
    Bodysuit,
    Vest,
    Overalls,
    Jacket,
    Kimono,
    Dress,
    Haori,
    /// Waistcoat over a shirt: torso `color`, sleeves `alt` (the shirt), buttons `trim`.
    Waistcoat,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct Top {
    pub cut: Cut,
    pub color: Col,
    /// Secondary colour (undershirt, overalls, side panels, coat lining…).
    #[serde(default)]
    pub alt: Option<Col>,
    /// Trim (collar, cuffs, buttons, tie).
    #[serde(default)]
    pub trim: Option<Col>,
    /// Bodysuit: torso side panels / outer arm colour.
    #[serde(default)]
    pub sides: Option<Col>,
    /// Horizontal stripes (every other row) on the torso and long sleeves.
    #[serde(default)]
    pub stripes: Option<Col>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Sleeves {
    Long,
    Short,
    Bare,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub enum Hands {
    Bare,
    Gloves(Col),
    Gauntlets(Col),
    Wraps(Col),
    Wristbands(Col),
    /// Fingerless gloves (glove colour on the back of the hand).
    Fingerless(Col),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum LegCut {
    Pants,
    Baggy,
    Shorts,
    Skirt,
    LongSkirt,
    Tights,
    /// Tights with briefs in `alt` (classic superhero).
    Trunks,
    Armored,
    Hakama,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct Legs {
    pub cut: LegCut,
    pub color: Col,
    #[serde(default)]
    pub alt: Option<Col>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum FootKind {
    Shoes,
    Boots,
    HighBoots,
    Sandals,
    Bare,
    Armored,
    Sneakers,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct Feet {
    pub kind: FootKind,
    pub color: Col,
    #[serde(default)]
    pub alt: Option<Col>,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub enum Belt {
    No,
    Belt(Col, Option<Col>),
    Sash(Col),
    Utility(Col),
}

/// Pixel-art decal: `rows[0]` is the top row; chars map through `pal` (`.`/space = skip).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Mosaic {
    pub rows: Vec<String>,
    pub pal: HashMap<char, Col>,
    /// Top row (voxel y); default depends on the surface.
    #[serde(default)]
    pub y: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum WingKind {
    Angel,
    Bat,
    Mech,
    Butterfly,
    Insect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum TailKind {
    Monkey,
    Cat,
    Demon,
    Fox,
    Lizard,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub enum Acc {
    /// Cape: colour, lining, high collar.
    Cape(Col, Option<Col>, bool),
    /// Short cloak over the shoulders (Link / ranger).
    Cloak(Col),
    Scarf(Col),
    Wings(WingKind, Col),
    Tail(TailKind, Col),
    /// Katana diagonally across the back (blade, hilt).
    BackSword(Col, Col),
    /// Huge sword on the back (blade, hilt).
    BigSword(Col, Col),
    /// Two crossed swords on the back.
    TwinSwords(Col, Col),
    /// `n` katanas on the left hip (sheath colours cycle).
    HipSwords(u8, Col),
    Staff(Col),
    /// Round shield on the back (face, rim, star/emblem colour).
    RoundShield(Col, Col, Option<Col>),
    /// Kite shield on the back (face, trim).
    KiteShield(Col, Col),
    Backpack(Col),
    Jetpack(Col, Col),
    Pauldron(Col, Side),
    Pauldrons(Col),
    Claws(Col),
    /// Big gauntlet on one hand (colour, gem colour).
    Gauntlet(Col, Col, Side),
    Pouches(Col),
    /// Lightsaber hilt at the hip (hilt colour).
    Saber(Col),
    Holsters(Col),
    Gourd(Col),
    Quiver(Col, Col),
    /// Hammer at the hip (head, handle).
    Hammer(Col, Col),
    /// Huge two-handed hammer on the back (head, handle): Reinhardt's rocket hammer.
    BigHammer(Col, Col),
    Necklace(Col),
    /// Glowing chest light (arc reactor) in colour.
    ChestLight(Col),
    Bow(Col),
    /// Rope/sash tails at the back of the waist.
    SashTails(Col),
    /// Spiky shoulder spikes.
    Spikes(Col),
    /// Round medallion / button at the collar.
    Collar(Col),
    /// Chain / belt of shells across the chest.
    Bandolier(Col),
    /// Fur/feather mantle over the shoulders.
    Mantle(Col),
    /// Scythe / polearm on the back.
    Polearm(Col, Col),
    /// Headset/earpiece etc. — tiny side light.
    Earpiece(Col),
    /// Battle axe on the back (head, handle).
    Axe(Col, Col),
    /// Turtle shell on the back (shell, rim).
    Shell(Col, Col),
    /// Arm cannon over a forearm (colour, muzzle glow).
    ArmCannon(Col, Col, Side),
}

/// Which colour the lobby ACCENT tweak replaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Zone {
    Top,
    Alt,
    Legs,
    Hair,
    Cape,
    Trim,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Build {
    Normal,
    Broad,
    Slim,
}

/// A look (or, for variants, a patch: set fields replace the base's).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Look {
    pub skin: Option<Col>,
    pub build: Option<Build>,
    pub hair: Option<Hair>,
    pub eyes: Option<Eyes>,
    pub brows: Option<Col>,
    pub mouth: Option<bool>,
    pub facial: Option<Vec<Facial>>,
    pub eyewear: Option<Eyewear>,
    pub mask: Option<Mask>,
    pub helmet: Option<Helmet>,
    pub hat: Option<Hat>,
    pub top: Option<Top>,
    pub sleeves: Option<Sleeves>,
    pub hands: Option<Hands>,
    pub legs: Option<Legs>,
    pub feet: Option<Feet>,
    pub belt: Option<Belt>,
    pub emblem: Option<Mosaic>,
    pub back_emblem: Option<Mosaic>,
    pub face_paint: Option<Mosaic>,
    pub acc: Option<Vec<Acc>>,
    /// Patch-only: appended to the accessories.
    pub add: Option<Vec<Acc>>,
    pub accent: Option<Zone>,
}

impl Look {
    /// `self` with every field `patch` sets replaced (`add` appends to `acc`).
    pub fn patched(&self, patch: &Look) -> Look {
        macro_rules! pick {
            ($($f:ident),*) => { Look { $($f: patch.$f.clone().or_else(|| self.$f.clone()),)* acc: None, add: None } };
        }
        let mut out = pick!(
            skin,
            build,
            hair,
            eyes,
            brows,
            mouth,
            facial,
            eyewear,
            mask,
            helmet,
            hat,
            top,
            sleeves,
            hands,
            legs,
            feet,
            belt,
            emblem,
            back_emblem,
            face_paint,
            accent
        );
        let mut acc = patch.acc.clone().or_else(|| self.acc.clone()).unwrap_or_default();
        acc.extend(self.add.clone().unwrap_or_default());
        acc.extend(patch.add.clone().unwrap_or_default());
        out.acc = Some(acc);
        out
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct VariantDef {
    pub name: String,
    /// Extra words that pick this variant when typed with the name ("gojo sunglasses").
    #[serde(default)]
    pub words: Vec<String>,
    #[serde(default)]
    pub look: Look,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct CharacterDef {
    /// Lowercase lookup names; the first is canonical.
    pub names: Vec<String>,
    #[serde(default)]
    pub source: String,
    pub look: Look,
    #[serde(default)]
    pub variants: Vec<VariantDef>,
}

impl CharacterDef {
    pub fn variant_count(&self) -> usize {
        self.variants.len().max(1)
    }
    /// Merged look of variant `i` (wraps).
    pub fn look_of(&self, i: usize) -> Look {
        match self.variants.get(i % self.variant_count()) {
            Some(v) => self.look.patched(&v.look),
            None => self.look.patched(&Look::default()),
        }
    }
}

// ------------------------------------------------------------------------------------ table

pub const CHARACTERS_RON: &str = include_str!("../../../../assets/costumes/characters.ron");

pub struct Table {
    pub chars: Vec<CharacterDef>,
    /// Normalised alias → index.
    pub index: HashMap<String, usize>,
    /// Parse error (tests assert it is `None`).
    pub error: Option<String>,
}

/// Parse a characters file.
pub fn parse_table(src: &str) -> Table {
    let opts = ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME);
    let (chars, error) = match opts.from_str::<Vec<CharacterDef>>(src) {
        Ok(c) => (c, None),
        Err(e) => (Vec::new(), Some(e.to_string())),
    };
    let mut index = HashMap::new();
    for (i, c) in chars.iter().enumerate() {
        for n in &c.names {
            let k = key(&tokens(n));
            if !k.is_empty() {
                index.entry(k).or_insert(i);
            }
        }
    }
    Table { chars, index, error }
}

/// The compiled-in character table.
pub fn table() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| parse_table(CHARACTERS_RON))
}

/// Lowercase alphanumeric tokens (apostrophes dropped: "luffy's" → "luffys").
pub fn tokens(s: &str) -> Vec<String> {
    s.to_lowercase()
        .replace(['\'', '’', '.'], "")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

fn key(toks: &[String]) -> String {
    toks.concat()
}

/// Words that may accompany a character name without changing the match.
pub(crate) const FILLER: &[&str] = &[
    "the", "a", "an", "as", "in", "with", "his", "her", "their", "mode", "form", "style", "outfit", "costume", "look", "suit", "version",
    "cosplay", "skin",
];

/// A phrase matched to a character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Found {
    pub index: usize,
    /// Variant picked by words in the phrase (0 = none named).
    pub base_variant: usize,
}

/// Find the character a costume phrase names: the whole phrase is an alias, or an alias plus variant
/// words / filler ("gojo sunglasses", "goku super saiyan", "the batman").
pub fn find(phrase: &str) -> Option<Found> {
    find_in(table(), phrase)
}

pub fn find_in(t: &Table, phrase: &str) -> Option<Found> {
    let toks = tokens(phrase);
    if toks.is_empty() {
        return None;
    }
    if let Some(&i) = t.index.get(&key(&toks)) {
        return Some(Found { index: i, base_variant: 0 });
    }
    let n = toks.len();
    // Longest alias window first.
    for len in (1..n).rev() {
        for s in 0..=n - len {
            let Some(&i) = t.index.get(&key(&toks[s..s + len])) else { continue };
            let c = &t.chars[i];
            let rest: Vec<&String> = toks[..s].iter().chain(&toks[s + len..]).filter(|w| !FILLER.contains(&w.as_str())).collect();
            if rest.is_empty() {
                return Some(Found { index: i, base_variant: 0 });
            }
            // Variant whose words cover every leftover word.
            let rest_key: String = rest.iter().map(|s| s.as_str()).collect();
            for (vi, v) in c.variants.iter().enumerate() {
                let mut words: Vec<String> = tokens(&v.name);
                for w in &v.words {
                    words.extend(tokens(w));
                }
                let phrase_words: Vec<String> =
                    v.words.iter().map(|w| key(&tokens(w))).chain(std::iter::once(key(&tokens(&v.name)))).collect();
                if rest.iter().all(|w| words.contains(w)) || phrase_words.contains(&rest_key) {
                    return Some(Found { index: i, base_variant: vi });
                }
            }
        }
    }
    None
}

/// Number of variants the phrase's character has (None = not a character phrase).
pub fn variant_count(phrase: &str) -> Option<usize> {
    find(phrase).map(|f| table().chars[f.index].variant_count())
}

/// A character's canonical name for RANDOMIZE (`r` picks; deterministic).
pub fn random_name(r: u32) -> Option<&'static str> {
    let t = table();
    if t.chars.is_empty() {
        return None;
    }
    t.chars[r as usize % t.chars.len()].names.first().map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_parse() {
        assert_eq!(parse_col("#ff8000"), Some([255, 128, 0]));
        assert_eq!(parse_col("f80"), Some([255, 136, 0]));
        assert_eq!(parse_col("black"), Some([22, 22, 26]));
        assert_eq!(parse_col("nope"), None);
    }

    #[test]
    fn tokens_normalise() {
        assert_eq!(key(&tokens("Spider-Man")), "spiderman");
        assert_eq!(key(&tokens("spider man")), "spiderman");
        assert_eq!(key(&tokens("Luffy's")), "luffys");
    }
}
