// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/CodeGen.scala
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//! A class file's bytes with the `TASTY` attribute, the UUID of the class's TASTy, and the
//! `Scala` marker (dotty's `genTastyAndSetAttributes`, `pickleMarkerForeign`).

use classfile::ClassFile;
use lower::{Lower, Unsupported};
use syntax::ast::Span;
use tasty::SymbolId;
use typer::pickle::Pickled;

/// Refuses unit `l` unless `pickles`, the TASTy of its top-level classes, declare the super
/// accessors its lowering made, each by its class and name.
pub fn check_super_accessors(l: &Lower, pickles: &[Pickled]) -> lower::Result<()> {
    let pickled: Vec<(SymbolId, String)> = pickles.iter().flat_map(|p| p.super_accessors.clone()).collect();
    let made: Vec<(SymbolId, String)> = l
        .super_accessors()
        .filter_map(|a| Some((l.u.symbol(a).owner?, l.u.symbol(a).name.clone())))
        .collect();
    if made.len() != pickled.len() || made.iter().any(|a| !pickled.contains(a)) {
        let what = "super accessors the TASTy lacks".to_owned();
        return Err(Unsupported { at: Span::NONE, what });
    }
    Ok(())
}

/// A class file: the class's internal name, its bytes and the TASTy its `TASTY` attribute names.
pub struct ClassBytes {
    pub name: String,
    pub bytes: Vec<u8>,
    pub tasty: Option<Vec<u8>>,
}

impl ClassBytes {
    /// Class `cf` named `name` with the `TASTY` attribute of `tasty` if given and the `Scala`
    /// marker.
    pub fn new(name: String, cf: &mut ClassFile, tasty: Option<Vec<u8>>) -> Result<Self, String> {
        if let Some(tasty) = &tasty {
            let header =
                tasty::Tasty::parse(tasty).map_err(|e| format!("our TASTy unreadable: {e:?}"))?.header;
            cf.custom_attribute("TASTY", header.uuid.to_vec());
        }
        cf.custom_attribute("Scala", Vec::new());
        Ok(ClassBytes { name, bytes: cf.bytes()?, tasty })
    }
}
