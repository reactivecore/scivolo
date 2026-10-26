// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nir/src/main/scala/scala/scalanative/nir/Mangle.scala
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenName.scala
//! Scala Native's mangled member names (`nir/Mangle.scala`, nscplugin's `NirGenName`) of
//! methods, from their erased types.

use super::Erased;
use crate::symbols::encode;
use crate::tags::{ERASED, INLINE, PRIVATE};
use crate::types::{Type, TypeId};
use crate::{Binder, Kind, SymbolId, Universe};

/// `id` as a mangled identifier: its length in UTF-16 units, a `-` if it starts with a digit or
/// `-`, then itself.
fn ident(id: &str) -> String {
    let dash = if id.starts_with(|c: char| c.is_ascii_digit() || c == '-') { "-" } else { "" };
    format!("{}{dash}{id}", id.encode_utf16().count())
}

pub fn mangle(e: &Erased) -> String {
    match e {
        Erased::Prim('R') => "R_".into(),
        Erased::Prim(c) => c.to_string(),
        Erased::Unit => "u".into(),
        Erased::Nothing => "n".into(),
        Erased::Null => "l".into(),
        Erased::Class(name) => format!("L{}", ident(name)),
        Erased::Array(elem) => format!("LA{}_", mangle(elem)),
        Erased::Value(_, underlying) => mangle(underlying),
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
            Some(o) if s.name == "+" => self.full_name(o) == "java.lang.String",
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

    /// The erased parameter types of method `def`, `Unit` boxed, and its erased result type, also
    /// boxed for a value; a type parameter erases to its upper bound.
    pub fn erased_method(&mut self, def: SymbolId) -> (Vec<Erased>, Erased) {
        self.erased_method_as(def, false)
    }

    /// `erased_method` with value classes semi-erased (`erase_value`).
    pub fn erased_method_value(&mut self, def: SymbolId) -> (Vec<Erased>, Erased) {
        self.erased_method_as(def, true)
    }

    /// The erasure of method `def`, as Scala 2 erases it for a method of a Scala 2 class that is
    /// not inline (dotty's `SourceLanguage`).
    fn erased_method_as(&mut self, def: SymbolId, semi: bool) -> (Vec<Erased>, Erased) {
        if let Some(e) = self.erased_memo(def, semi) {
            return e.clone();
        }
        let generation = self.generation();
        let owner = self.symbol(def).owner;
        let in_scala2_class = owner.is_some_and(|o| self.symbol(o).kind == Kind::Class && self.is_scala2(o));
        let scala2 = in_scala2_class && !self.symbol(def).flags.has(INLINE);
        let previous = self.set_scala2_erasure(scala2);
        let erased = self.erase_method(def, semi);
        self.set_scala2_erasure(previous);
        self.remember_erased((def, semi), generation, &erased);
        erased
    }

    fn erase_method(&mut self, def: SymbolId, semi: bool) -> (Vec<Erased>, Erased) {
        let erase = |u: &mut Self, t| if semi { u.erase_value(t) } else { u.erase(t) };
        let mut t = self.info(def);
        let mut params = Vec::new();
        let mut bounds = Vec::new();
        loop {
            match self.types().get(t).clone() {
                Type::Poly { binder, params: ps, result } => {
                    bounds.push((binder, ps.into_iter().map(|(_, b)| b).collect::<Vec<_>>()));
                    t = result;
                }
                Type::Method { params: ps, result, .. } => {
                    params.extend(ps.into_iter().map(|(_, p)| p));
                    t = result;
                }
                _ => break,
            }
        }
        params.retain(|&p| !self.is_erased_param(p));
        let params = params
            .into_iter()
            .map(|p| {
                let p = self.upper_bound(p, &bounds);
                match erase(self, p) {
                    Erased::Unit => Erased::Class("scala.runtime.BoxedUnit".into()),
                    e => e,
                }
            })
            .collect();
        let t = self.upper_bound(t, &bounds);
        match erase(self, t) {
            Erased::Unit if self.symbol(def).kind == Kind::Val => {
                (params, Erased::Class("scala.runtime.BoxedUnit".into()))
            }
            e => (params, e),
        }
    }

    /// `t` with its type parameters replaced by their upper bounds, given the bounds of the
    /// parameters of each polymorphic type; within `t`, only those bounded by a class.
    fn upper_bound(&mut self, t: TypeId, bounds: &[(Binder, Vec<TypeId>)]) -> TypeId {
        let bound =
            |b: Binder, i: u32| bounds.iter().find(|(binder, _)| *binder == b)?.1.get(i as usize).copied();
        let mut t = self.map_type(t, &mut |u, x| match *u.types().get(x) {
            Type::Param(b, i) => {
                let bounds = bound(b, i)?;
                let Type::Bounds(_, hi) = *u.types().get(bounds) else { return None };
                (!u.is_generic(bounds)).then_some(hi)
            }
            _ => None,
        });
        while let Type::Param(b, i) = *self.types().get(t) {
            let Some(bound) = bound(b, i) else { break };
            t = match *self.types().get(bound) {
                Type::Bounds(_, hi) => hi,
                _ => bound,
            };
        }
        t
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
