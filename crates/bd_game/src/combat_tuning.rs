//! Combat tuning (`assets/tuning/combat.ron`, bd_sim::tuning::CombatTuning, COMBAT.md §5i) for the
//! authoritative sim.
//!
//! * [`current`]: the tuning every locally built authoritative world starts with (practice, host,
//!   headless host, soak / showcase / bench). Loaded from the file on first use; a missing or
//!   invalid file logs a warning and falls back to the built-in defaults (never crashes).
//! * [`CombatTuningPlugin`]: hot reload (mtime poll every 0.5 s, like `move.ron`). A valid edit
//!   replaces [`current`] and is applied with `World::set_tuning` to the running authoritative
//!   world (practice `Offline` / listen-server `Host`); the host also sends it to every client
//!   (`S2C::Tuning`). A parse error keeps the previous tuning.
//! * Clients never read their own file while connected: their replica / prediction world is built
//!   from the host's `WorldInit.cfg` (which carries the host's tuning) and follows `S2C::Tuning`.

use crate::assets_dir;
use crate::sim_bridge::{ActiveWorld, SimRole};
use bd_core::mechanics::Form;
use bd_sim::tuning::CombatTuning;
use bevy::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::SystemTime;

/// Latest successfully loaded tuning (`None` = not loaded yet).
static CURRENT: RwLock<Option<CombatTuning>> = RwLock::new(None);

/// `assets/tuning/combat.ron`.
pub fn path() -> PathBuf {
    assets_dir().join("tuning").join("combat.ron")
}

/// Read, parse and validate a tuning file.
pub fn load_file(path: &Path) -> Result<CombatTuning, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
    CombatTuning::from_ron(&src).map_err(|e| format!("can't parse {}: {e}", path.display()))
}

/// Load `path`, or the built-in defaults (with a warning) if it is missing or invalid.
pub fn load_or_default(path: &Path) -> CombatTuning {
    match load_file(path) {
        Ok(t) => {
            info!("combat tuning: loaded {} ({})", path.display(), summary(&t));
            t
        }
        Err(e) => {
            warn!("combat tuning: {e}; using the built-in defaults");
            eprintln!("combat tuning: {e}; using the built-in defaults");
            CombatTuning::default()
        }
    }
}

/// The tuning new authoritative worlds use (loads `combat.ron` on first call).
pub fn current() -> CombatTuning {
    if let Some(t) = CURRENT.read().ok().and_then(|g| g.clone()) {
        return t;
    }
    let t = load_or_default(&path());
    if let Ok(mut g) = CURRENT.write() {
        g.get_or_insert_with(|| t.clone());
    }
    t
}

fn set_current(t: &CombatTuning) {
    if let Ok(mut g) = CURRENT.write() {
        *g = Some(t.clone());
    }
}

/// Log line: the most-tuned knobs, so a reload is visible in the log.
pub fn summary(t: &CombatTuning) -> String {
    format!(
        "form cost Gun x{:.2} Staff x{:.2}, ring blast dmg {:.1}, regen {:.2}/s",
        t.willpower.form_mult(Form::Gun),
        t.willpower.form_mult(Form::Staff),
        t.ring.blast_damage,
        t.willpower.regen_per_sec
    )
}

pub struct CombatTuningPlugin;

impl Plugin for CombatTuningPlugin {
    fn build(&self, app: &mut App) {
        // Load eagerly so a bad file is reported at startup (not on the first match).
        let _ = current();
        app.add_systems(Update, hot_reload);
    }
}

fn hot_reload(
    time: Res<Time<Real>>,
    active: Option<ResMut<ActiveWorld>>,
    net: Option<ResMut<crate::net::NetState>>,
    mut last: Local<(f32, Option<SystemTime>)>,
) {
    last.0 += time.delta_secs();
    if last.0 < 0.5 {
        return;
    }
    last.0 = 0.0;
    let p = path();
    let Ok(modified) = std::fs::metadata(&p).and_then(|m| m.modified()) else { return };
    match last.1 {
        None => last.1 = Some(modified),
        Some(prev) if prev != modified => {
            last.1 = Some(modified);
            let t = match load_file(&p) {
                Ok(t) => t,
                Err(e) => {
                    warn!("combat tuning: {e}; keeping the previous tuning");
                    return;
                }
            };
            set_current(&t);
            let mut active = active;
            let applied = match active.as_deref().map(|a| a.role) {
                Some(SimRole::Client) => "ignored: connected to a host (its tuning is authoritative)",
                Some(SimRole::Offline) => {
                    if let Some(a) = active.as_deref_mut() {
                        a.world.set_tuning(t.clone());
                    }
                    "practice world"
                }
                Some(SimRole::Host) | None => "next match",
            };
            // Host: apply to the match world and send it to every client (lobby clients also get
            // it with their next WorldInit).
            let mut applied = applied.to_string();
            if let Some(h) = net.and_then(|n| n.into_inner().host.as_mut()) {
                let world = active.as_deref_mut().filter(|a| a.role == SimRole::Host).map(|a| &mut a.world);
                let clients = h.set_tuning(world, &t);
                applied = format!("host world + {clients} client(s)");
            }
            info!("combat tuning reloaded ({}) -> {applied}", summary(&t));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_combat_ron_loads_with_the_balance_knobs() {
        let t = load_file(&path()).expect("assets/tuning/combat.ron parses");
        assert_eq!(t.willpower.form_mult(Form::Gun), 2.6, "Gun form cost from the shipped file");
        assert_eq!(t.willpower.form_mult(Form::Staff), 2.0);
        assert_eq!(t, CombatTuning::default(), "shipped file == Rust defaults");
        // What practice / host worlds start with.
        assert_eq!(current().willpower.form_mult(Form::Gun), 2.6);
        let cfg = crate::practice::practice_config(1, 1);
        assert_eq!(cfg.tuning.willpower.form_mult(Form::Gun), 2.6);
    }

    #[test]
    fn missing_or_invalid_files_fall_back_to_defaults() {
        let dir = std::env::temp_dir().join(format!("bd_combat_tuning_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let missing = dir.join("nope.ron");
        assert!(load_file(&missing).is_err());
        assert_eq!(load_or_default(&missing), CombatTuning::default());
        let bad = dir.join("bad.ron");
        std::fs::write(&bad, "( willpower: ( form_cost: [ (form: Gun, mult: ").unwrap();
        assert!(load_file(&bad).is_err());
        assert_eq!(load_or_default(&bad), CombatTuning::default());
        // Partial + hostile values: defaults fill in, validation clamps.
        let partial = dir.join("partial.ron");
        std::fs::write(&partial, "(willpower: (form_cost: [(form: Gun, mult: 3.0)], regen_per_sec: -4.0))").unwrap();
        let t = load_file(&partial).unwrap();
        assert_eq!(t.willpower.form_mult(Form::Gun), 3.0);
        assert_eq!(t.willpower.form_mult(Form::Staff), 1.0, "an explicit list replaces the default knobs");
        assert!(t.willpower.regen_per_sec >= 0.0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
