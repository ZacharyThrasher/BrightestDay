use bd_sim::powers::toolbox::MoveIndex;
use bd_sim::powers::{norm_words, PowerTable};
#[test]
fn dbg() {
    let t = PowerTable::get();
    let ix = MoveIndex::shipped();
    for (k, p) in ix.kit_names().filter(|(k, _)| *k == "minotaur") {
        eprintln!("kit {k} -> {} {}", p, t.powers[p as usize].id);
    }
    let r = ix.kit_move(t, &norm_words("minotaur combo"));
    eprintln!("{r:?} {:?}", r.map(|(m, _)| t.powers[m.power as usize].id.clone()));
    let h = t.powers.iter().position(|p| p.id == "myth_minotaur").unwrap();
    eprintln!("hand moves: {:?}", t.powers[h].moves.iter().map(|m| (&m.name, &m.tags)).collect::<Vec<_>>());
}
