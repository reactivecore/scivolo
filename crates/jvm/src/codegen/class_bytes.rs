// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/CodeGen.scala
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//   compiler/src/dotty/tools/backend/jvm/BCodeSkelBuilder.scala
//! A class file's bytes with the `Scala` marker, the `Scala3InlineInfo` attribute and the
//! `TASTY` attribute, the UUID of the class's TASTy (dotty's `genTastyAndSetAttributes`,
//! `pickleMarkerForeign`, `initJClass`).

use super::inline_info::{SCALA3_INLINE_INFO, empty_inline_info};
use classfile::{ClassFile, Lub};
use lower::{Lower, Unsupported};
use syntax::ast::Span;
use tasty::SymbolId;
use typer::pickle::Pickled;

/// The prefix of the internal names of Scala Native's library.
const SCALA_NATIVE_PACKAGE: &str = "scala/scalanative/";

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

/// Refuses class `cf`, defined at `at`, if it refers to a class of Scala Native's library that no
/// jar of the class path has.
pub fn check_native_references(l: &mut Lower, cf: &ClassFile, at: Span) -> lower::Result<()> {
    let references = cf.referenced_classes_where(|name| name.starts_with(SCALA_NATIVE_PACKAGE));
    match references.into_iter().find(|name| !l.u.has_class_file(name)) {
        Some(name) => {
            let what =
                format!("Scala Native's `{}` without its jar on the class path", name.replace('/', "."));
            Err(Unsupported { at, what })
        }
        None => Ok(()),
    }
}

/// Adds the `Scala` marker to `cf`, then the `Scala3InlineInfo` attribute unless `cf` is a
/// mirror class.
pub fn scala_markers(cf: &mut ClassFile, mirror: bool) {
    cf.custom_attribute("Scala", Vec::new());
    if !mirror {
        cf.custom_attribute(SCALA3_INLINE_INFO, empty_inline_info());
    }
}

/// A class file: the class's internal name, its bytes and the TASTy its `TASTY` attribute names.
pub struct ClassBytes {
    pub name: String,
    pub bytes: Vec<u8>,
    pub tasty: Option<Vec<u8>>,
}

impl ClassBytes {
    /// Class `cf` named `name` with the `TASTY` attribute of `tasty` if given; `lub` gives the
    /// common superclass of two classes for its stack map frames.
    pub fn new(
        name: String,
        mut cf: ClassFile,
        tasty: Option<Vec<u8>>,
        lub: &mut Lub,
    ) -> Result<Self, String> {
        if let Some(tasty) = &tasty {
            let header =
                tasty::Tasty::parse(tasty).map_err(|e| format!("our TASTy unreadable: {e:?}"))?.header;
            cf.custom_attribute("TASTY", header.uuid.to_vec());
        }
        Ok(ClassBytes { name, bytes: cf.bytes(lub)?, tasty })
    }
}
