// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/opt/InlineInfoAttribute.scala
//   compiler/src/dotty/tools/backend/jvm/BTypeLoader.scala
//! The `Scala3InlineInfo` attribute of a class (dotty's `InlineInfoAttribute.write`).

/// The attribute's name.
pub const SCALA3_INLINE_INFO: &str = "Scala3InlineInfo";

/// The version of the attribute's layout.
const VERSION: u8 = 1;

/// Determines the body of the `Scala3InlineInfo` attribute without the optimizer's inliner:
/// `InlineInfo.empty`, not final, without a SAM and methods (dotty's `buildInlineInfo` gives it
/// unless `-opt:inline` is set).
pub fn empty_inline_info() -> Vec<u8> {
    let (flags, methods) = (0u8, 0u16);
    let mut out = vec![VERSION, flags];
    out.extend(methods.to_be_bytes());
    out
}
