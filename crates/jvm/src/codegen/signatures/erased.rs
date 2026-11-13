// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala
//! Generic signatures of members known only by their erased descriptors.

use super::{Boxing, Member, Sig};
use tasty::{SymbolId, Type, Universe};

/// Splits descriptor `desc` into the class types it names (`Lp/C;`, `true`) and the text between.
pub fn descriptor_parts(desc: &str) -> Vec<(&str, bool)> {
    let mut parts = Vec::new();
    let mut rest = desc;
    while let Some(start) = rest.find('L') {
        let Some(end) = rest[start..].find(';').map(|e| start + e + 1) else { break };
        parts.push((&rest[..start], false));
        parts.push((&rest[start..end], true));
        rest = &rest[end..];
    }
    parts.push((rest, false));
    parts
}

/// Builds the generic signature of member `m` known only by its descriptor `desc` (dotty's
/// `javaSig` of an erased info), `classes` the classes of `desc`'s class types in order: a member
/// class of a generic class as selected from that class applied to its type parameters, which
/// are erased unless in scope; none if it equals `desc`.
pub fn erased_member(
    u: &mut Universe,
    m: Member,
    desc: &str,
    classes: &[Option<SymbolId>],
) -> Option<String> {
    let mut s = Sig::new(u, m.sym, m.top);
    s.origin = m.origin;
    s.extended = m.extended;
    let mut classes = classes.iter().copied();
    for (text, class) in descriptor_parts(desc) {
        match class.then(|| classes.next().flatten()).flatten() {
            Some(c) => s.member_class(c, text),
            None => s.out.push_str(text),
        }
    }
    (s.out != desc).then_some(s.out)
}

impl Sig<'_> {
    /// Appends class `c` of erased type `plain`, selected from its owner applied to its type
    /// parameters if that owner is a generic class (dotty's `classSig` of `c.typeRef`).
    fn member_class(&mut self, c: SymbolId, plain: &str) {
        let owner = self.u.symbol(c).owner;
        let generic = owner.filter(|&o| self.u.is_class(o) && !self.u.type_params(o).is_empty());
        let Some(o) = generic else { return self.out.push_str(plain) };
        let prefix = self.u.this_type(o);
        let t = self.u.intern_type(Type::Sym(Some(prefix), c));
        self.reference(t, Boxing::Box);
    }
}
