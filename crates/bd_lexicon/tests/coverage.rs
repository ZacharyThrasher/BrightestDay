//! Coverage report: how many of ~300 words players plausibly type resolve to something meaningful.
//!     CARGO_TARGET_DIR=target/lexicon cargo test -p bd_lexicon --test coverage -- --nocapture

mod common;

use bd_core::mechanics::Form;

const WORDS: &str = "
hammer sword axe spear shield gauntlet whip gun bow cannon staff disc wings parachute jetpack cape
maul mallet sledgehammer gavel club mace flail katana rapier scimitar dagger knife machete scythe sickle
halberd pike lance trident harpoon javelin boomerang frisbee chakram shuriken crossbow slingshot catapult
bazooka musket rifle pistol shotgun minigun flamethrower laser blaster wand scepter broom umbrella kite
balloon glider feather cloak robe helmet armor boots glove fist claw talon tentacle tail horn fang
dragon griffin phoenix unicorn pegasus hydra kraken cyclops minotaur centaur golem troll ogre goblin orc
zombie vampire werewolf ghost skeleton mummy demon angel fairy wizard witch ninja pirate knight samurai
viking cowboy robot cyborg android alien ufo astronaut clown mime chicken cow pig horse donkey goat sheep
dog cat mouse rat rabbit squirrel bear wolf fox lion tiger cheetah leopard panther elephant rhino hippo
giraffe zebra camel kangaroo koala panda gorilla monkey sloth bat eagle hawk owl crow raven parrot
penguin ostrich flamingo pelican duck goose swan shark whale dolphin octopus squid jellyfish crab lobster
shrimp eel piranha salmon trout snake python cobra viper lizard gecko iguana crocodile alligator turtle
tortoise frog toad scorpion spider tarantula ant bee wasp hornet beetle butterfly moth mosquito fly
dinosaur trex velociraptor triceratops stegosaurus pterodactyl mammoth yeti bigfoot kaiju
car truck bus train tank helicopter airplane jet rocket missile submarine boat ship canoe bicycle
motorcycle skateboard surfboard sled tractor bulldozer steamroller forklift ambulance firetruck
wall fence gate door ramp stairs ladder bridge tower pillar column dome tent igloo cage jail trampoline
platform castle house barn skyscraper lighthouse windmill tree cactus mushroom flower rock boulder
mountain volcano meteor comet asteroid moon planet sun star cloud tornado lightning fire ice water wind
bomb grenade dynamite tnt mine firework anvil piano safe fridge toaster microwave couch chair table bed
lamp spoon fork pan pot kettle bucket barrel crate box bottle cup plate book pencil phone computer
television guitar drum trumpet violin tuba pizza banana apple watermelon cake pie donut burger taco
mjolnir excalibur lightsaber pokeball batarang pikachu charizard snorlax gyarados mewtwo onix eevee
godzilla mothra kong cthulhu kirby yoshi mario luigi bowser sonic goku shrek groot hulk thor batman
spiderman superman eeyore totoro creeper xenomorph terminator dalek gundam optimus
rage love fear chaos justice sadness
magnet chain rope net web glue slime lava
";

#[test]
fn coverage_report() {
    let Some(lex) = common::lex() else { return };
    let words: Vec<&str> = WORDS.split_whitespace().collect();
    let mut hits = 0;
    let mut misses = Vec::new();
    let mut weak = Vec::new();
    let mut icons = 0;
    for w in &words {
        let r = lex.resolve(w);
        let meaningful = r.recipe.form != Form::Blob0 || !r.graft.mechanics.is_empty() || r.graft.element.is_some();
        if meaningful && r.confidence >= 0.3 {
            hits += 1;
            if r.shape.is_some() {
                icons += 1;
            }
        } else if meaningful {
            weak.push(format!("{w}({:.2})", r.confidence));
        } else {
            misses.push(*w);
        }
    }
    let rate = hits as f64 / words.len() as f64;
    println!("\n=== bd_lexicon coverage: {hits}/{} words = {:.1}% (icons: {icons}) ===", words.len(), rate * 100.0);
    println!("low confidence: {weak:?}");
    println!("misses: {misses:?}");
    assert!(words.len() >= 300, "word list has {}", words.len());
    assert!(rate >= 0.95, "coverage {:.1}% < 95%", rate * 100.0);
}
