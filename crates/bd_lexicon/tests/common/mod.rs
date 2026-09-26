use std::sync::OnceLock;

use bd_lexicon::Lexicon;

/// The real lexicon (assets/lexicon.bin + icons). `None` if the asset hasn't been built yet
/// (run `python tools/lexicon_build/build.py`); tests then skip with a message.
pub fn lex() -> Option<&'static Lexicon> {
    static LEX: OnceLock<Option<Lexicon>> = OnceLock::new();
    LEX.get_or_init(|| {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/lexicon.bin");
        match Lexicon::load(&p) {
            Ok(l) => Some(l),
            Err(e) => {
                eprintln!("SKIP: cannot load {}: {e}", p.display());
                None
            }
        }
    })
    .as_ref()
}
