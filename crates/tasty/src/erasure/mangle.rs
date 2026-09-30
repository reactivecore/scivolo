//! Scala Native's mangled member names (`nir/Mangle.scala`, nscplugin's `NirGenName`) of
//! methods, from their erased types.

use super::Erased;
use crate::symbols::encode;
use crate::tags::{ERASED, INLINE, PRIVATE};
use crate::types::Type;
use crate::{Kind, SymbolId, Universe};

/// `id` as a mangled identifier: its length in UTF-16 units, a `-` if it starts with a digit or
/// `-`, then itself.
fn ident(id: &str) -> String {
    let dash = if id.starts_with(|c: char| c.is_ascii_digit() || c == '-') { "-" } else { "" };
    format!("{}{dash}{id}", id.encode_utf16().count())
}

fn mangle(e: &Erased) -> String {
    match e {
        Erased::Prim(c) => c.to_string(),
        Erased::Unit => "u".into(),
        Erased::Nothing => "n".into(),
        Erased::Null => "l".into(),
        Erased::Class(name) => format!("L{}", ident(name)),
        Erased::Array(elem) => format!("LA{}_", mangle(elem)),
    }
}

impl Universe {
    /// The mangled signature of public method `def`, e.g. `D3mapL15scala.Function1L12scala.OptionEO`,
    /// under its `@targetName` if it has one; `None` unless it is a public instance method that
    /// exists at run time: not erased, not a super accessor, not `inline` unless it overrides a
    /// method that is not.
    pub fn method_signature(&mut self, def: SymbolId) -> Option<String> {
        let s = self.symbol(def);
        let runtime = !s.flags.has(PRIVATE) && !s.flags.has(ERASED);
        if s.kind != Kind::Def || !runtime || s.name == "<init>" || s.name.starts_with("super$") {
            return None;
        }
        if s.flags.has(INLINE) && !self.overrides_plain(def) {
            return None;
        }
        let s = self.symbol(def);
        let name = encode(s.target.as_deref().unwrap_or(&s.name));
        let mut t = self.info(def);
        let mut params = Vec::new();
        loop {
            match self.types().get(t).clone() {
                Type::Poly { result, .. } => t = result,
                Type::Method { params: ps, result, .. } => {
                    params.extend(ps.into_iter().map(|(_, p)| p));
                    t = result;
                }
                _ => break,
            }
        }
        let mut out = format!("D{}", ident(&name));
        for p in params {
            let e = match self.erase(p) {
                Erased::Unit => Erased::Class("scala.runtime.BoxedUnit".into()),
                e => e,
            };
            out.push_str(&mangle(&e));
        }
        let result = self.erase(t);
        out.push_str(&mangle(&result));
        out.push_str("EO");
        Some(out)
    }
}

impl Universe {
    /// Whether `def` overrides a method of the same name that is not `inline` in a base class of
    /// its owner.
    fn overrides_plain(&mut self, def: SymbolId) -> bool {
        let Some(owner) = self.symbol(def).owner.filter(|&o| self.symbol(o).kind == Kind::Class) else {
            return false;
        };
        let name = self.symbol(def).name.clone();
        self.linearization(owner).into_iter().skip(1).any(|base| {
            self.decl(base, &name)
                .unwrap_or_default()
                .into_iter()
                .any(|m| self.symbol(m).kind == Kind::Def && !self.symbol(m).flags.has(INLINE))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Erased, ident, mangle};

    #[test]
    fn mangles_like_scala_native() {
        assert_eq!(ident("map"), "3map");
        assert_eq!(ident("1st"), "3-1st");
        assert_eq!(mangle(&Erased::Class("scala.Option".into())), "L12scala.Option");
        assert_eq!(mangle(&Erased::Array(Box::new(Erased::Prim('i')))), "LAi_");
    }
}
