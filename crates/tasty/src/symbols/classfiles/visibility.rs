// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/classfile/ClassfileConstants.scala
//   compiler/src/dotty/tools/dotc/core/classfile/ClassfileParser.scala
//! The visibility of a Java class or member as dotty gives it (`FlagTranslation`,
//! `getPrivateWithin`): `private`, `protected[p]` or, package-private, `private[p]`, with `p` its
//! package.

use super::super::{Kind, SymbolId, Universe};
use crate::tags::{PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified};
use classfile::{ACC_PRIVATE, ACC_PROTECTED, ACC_PUBLIC};

/// Determines the modifiers of Java access flags `access`.
pub fn modifiers(access: u16) -> &'static [u8] {
    match access {
        _ if access & ACC_PUBLIC != 0 => &[],
        _ if access & ACC_PRIVATE != 0 => &[PRIVATE],
        _ if access & ACC_PROTECTED != 0 => &[PROTECTED, PROTECTEDqualified],
        _ => &[PRIVATEqualified],
    }
}

/// Gives `s`, entered with the `modifiers` of `access`, its package as access qualifier unless
/// it is public or private.
pub fn qualify(u: &mut Universe, s: SymbolId, access: u16) {
    if access & (ACC_PUBLIC | ACC_PRIVATE) != 0 {
        return;
    }
    let mut pkg = u.symbol(s).owner;
    while let Some(p) = pkg.filter(|&p| u.symbol(p).kind != Kind::Package) {
        pkg = u.symbol(p).owner;
    }
    u.set_access_qualifier(s, pkg);
}
