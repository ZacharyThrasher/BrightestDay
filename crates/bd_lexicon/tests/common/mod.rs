use std::sync::OnceLock;

use bd_lexicon::Lexicon;

/// The real lexicon (assets/lexicon.bin + icons). `None` if the asset hasn't been built yet
/// (run `python tools/lexicon_build/build.py`); tests then skip with a message.
pub fn lex() -> Option<&'static Lexicon> {
    static LEX: OnceLock<Option<Lexicon>> = OnceLock::new();
    LEX.get_or_init(|| {
        // BD_LEXICON_BIN: test a freshly packed file elsewhere (icons are still read from assets/)
        let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let p = std::env::var_os("BD_LEXICON_BIN").map(std::path::PathBuf::from).unwrap_or_else(|| assets.join("lexicon.bin"));
        if std::env::var_os("BD_LEXICON_BIN").is_some() {
            let bytes = std::fs::read(&p).ok()?;
            return Lexicon::from_parts(bytes, &Lexicon::icon_files(&assets), &Lexicon::power_files(&assets)).ok();
        }
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
