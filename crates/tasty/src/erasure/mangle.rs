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
        Erased::Prim('R') => "R_".into(),
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
    /// method that is not, not an extern, not a member of `Any`, `Object`'s `eq`, `ne` and
    /// `synchronized`, or `String`'s `+`.
    pub fn method_signature(&mut self, def: SymbolId) -> Option<String> {
        let s = self.symbol(def);
        let runtime = !s.flags.has(PRIVATE) && !s.flags.has(ERASED);
        if s.kind != Kind::Def || !runtime || s.name == "<init>" || s.name.starts_with("super$") {
            return None;
        }
        if s.is_extern || s.owner.is_some_and(|o| self.symbol(o).is_extern) {
            return None;
        }
        let builtins = self.builtins();
        let primitive = match s.owner {
            Some(o) if o == builtins.any => true,
            Some(o) if o == builtins.object => matches!(s.name.as_str(), "eq" | "ne" | "synchronized"),
            Some(o) => s.name == "+" && self.full_name(o) == "java.lang.String",
            _ => false,
        };
        if primitive {
            return None;
        }
        if s.flags.has(INLINE) && !self.overrides_plain(def) {
            return None;
        }
        let s = self.symbol(def);
        let name = encode(s.target.as_deref().unwrap_or(&s.name));
        let (params, result) = self.erased_method(def);
        let mut out = format!("D{}", ident(&name));
        for p in &params {
            out.push_str(&mangle(p));
        }
        out.push_str(&mangle(&result));
        out.push_str("EO");
        Some(out)
    }

    /// The erased parameter types of method `def`, `Unit` boxed, and its erased result type.
    pub fn erased_method(&mut self, def: SymbolId) -> (Vec<Erased>, Erased) {
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
        let params = params
            .into_iter()
            .map(|p| match self.erase(p) {
                Erased::Unit => Erased::Class("scala.runtime.BoxedUnit".into()),
                e => e,
            })
            .collect();
        (params, self.erase(t))
    }

    /// Whether `def` overrides a method of a base class of its owner that is neither `inline`
    /// nor private and has the same name and erased parameter types.
    fn overrides_plain(&mut self, def: SymbolId) -> bool {
        let Some(owner) = self.symbol(def).owner.filter(|&o| self.symbol(o).kind == Kind::Class) else {
            return false;
        };
        let name = self.symbol(def).name.clone();
        let (params, _) = self.erased_method(def);
        for base in self.linearization(owner).into_iter().skip(1) {
            for m in self.decl(base, &name).unwrap_or_default() {
                let f = self.symbol(m).flags;
                if self.symbol(m).kind == Kind::Def
                    && !f.has(INLINE)
                    && !f.has(PRIVATE)
                    && self.erased_method(m).0 == params
                {
                    return true;
                }
            }
        }
        false
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
