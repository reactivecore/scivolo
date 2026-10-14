//! The implicit scope of a type (dotty's `implicitScope`): the implicits of the companions of the
//! classes of its parts and of their base classes, and of the values on their prefix paths.

use super::super::Typer;
use super::candidates::Candidate;
use tasty::{Kind, SymbolId, Type, TypeId};

/// The implicits of an implicit scope, and the objects and values they are members of (dotty's
/// `companionRefs`) as references.
#[derive(Default)]
pub struct ImplicitScope {
    pub implicits: Vec<Candidate>,
    pub containers: Vec<Candidate>,
}

impl Typer<'_> {
    /// The implicits of the companions of the classes `t` refers to, those of the objects it
    /// refers to, those of their base classes, and those of the stable values their prefixes go
    /// through.
    pub fn implicit_scope(&mut self, t: TypeId) -> Vec<Candidate> {
        self.implicit_scope_of(t).implicits
    }

    pub fn implicit_scope_of(&mut self, t: TypeId) -> ImplicitScope {
        let t = self.zonk(t);
        let mut parts = Vec::new();
        self.collect_parts(t, &mut Vec::new(), &mut parts);
        let mut out = ImplicitScope::default();
        for (c, prefix) in parts {
            let owner = self.u.symbol(c).owner;
            for base in self.u.linearization(c) {
                let companion = match self.companion_module(base) {
                    Some(module) => Some((module, crate::defn::type_ref(self.u, module))),
                    None if self.u.symbol(base).owner == owner => self.inner_companion(base, prefix),
                    None => None,
                };
                let Some((module, module_ref)) = companion else { continue };
                let Some(module_class) = self.u.module_class(module) else { continue };
                self.add_members(module_class, module_ref, &mut out);
            }
            if let Some(p) = prefix {
                self.add_path(p, &mut out);
            }
        }
        out
    }

    /// Adds the implicit members of `class` selected from path `prefix`, and `prefix`, to `out`.
    fn add_members(&mut self, class: SymbolId, prefix: TypeId, out: &mut ImplicitScope) {
        for sym in self.implicit_members(class, false) {
            let candidate = Candidate { sym, prefix: Some(prefix) };
            if !out.implicits.contains(&candidate) {
                out.implicits.push(candidate);
            }
        }
        if let Type::Sym(p, sym) = *self.u.types().get(prefix)
            && !out.containers.contains(&Candidate { sym, prefix: p })
        {
            out.containers.push(Candidate { sym, prefix: p });
        }
    }

    /// The companion object of class `c` nested in a class other than an object's, as a member
    /// of path `prefix`.
    fn inner_companion(&mut self, c: SymbolId, prefix: Option<TypeId>) -> Option<(SymbolId, TypeId)> {
        let prefix = prefix.filter(|&p| matches!(self.u.types().get(p), Type::This(_) | Type::Sym(..)))?;
        let module = match self.u.is_module_class(c) {
            true => self.u.source_module(c),
            false => self.u.companion_module(c),
        }?;
        Some((module, self.u.intern_type(Type::Sym(Some(prefix), module))))
    }

    /// Adds the implicit members of the stable values that are not static objects on path `p` to
    /// `out` (dotty's `addPath`).
    fn add_path(&mut self, p: TypeId, out: &mut ImplicitScope) {
        let Type::Sym(outer, s) = *self.u.types().get(p) else { return };
        if !self.u.symbol(s).kind.is_term()
            || self.is_excluded(s)
            || self.is_static_term(s) && self.u.is_module(s)
        {
            return;
        }
        self.complete(s);
        let info = self.u.info(s);
        if matches!(self.u.types().get(info), Type::Sym(_, x) if self.u.symbol(*x).kind.is_term()) {
            return self.add_path(info, out);
        }
        let widened = self.u.widen(p);
        if let Some(class) = self.u.class_symbol(widened) {
            self.add_members(class, p, out);
        }
        if let Some(outer) = outer {
            self.add_path(outer, out);
        }
    }

    /// Adds to `parts` the classes of the parts of `t` with the prefixes they are referenced
    /// through (dotty's `collectParts`): of the references in `t` dealiased, of the types of its
    /// term references, and of the prefixes of its class, abstract type and term references; never
    /// of packages or package objects.
    fn collect_parts(
        &mut self,
        t: TypeId,
        seen: &mut Vec<TypeId>,
        parts: &mut Vec<(SymbolId, Option<TypeId>)>,
    ) {
        if seen.contains(&t) {
            return;
        }
        seen.push(t);
        let t = self.u.dealias(t);
        let (prefix, sym) = match self.u.types().get(t).clone() {
            Type::Sym(p, s) => (p, s),
            Type::TypeRef(p, n) | Type::TermRef(p, n) => {
                let ty = matches!(self.u.types().get(t), Type::TypeRef(..));
                let Some(s) = self.u.resolve(p, &n, ty) else { return };
                (Some(p), s)
            }
            Type::This(c) => return self.collect_parts(c, seen, parts),
            _ => {
                for part in self.children(t) {
                    self.collect_parts(part, seen, parts);
                }
                return;
            }
        };
        if self.is_excluded(sym) {
            return;
        }
        let class = match self.u.symbol(sym).kind.is_term() {
            true => {
                let widened = self.u.widen(t);
                let class = (widened != t).then(|| self.u.class_symbol(widened)).flatten();
                class.map(|c| (c, class_prefix(self.u, widened, c)))
            }
            false => self.u.class_symbol(t).map(|c| (c, prefix.filter(|_| c == sym))),
        };
        if let Some(c) = class.filter(|&(c, _)| !self.is_excluded(c))
            && !parts.contains(&c)
        {
            parts.push(c);
        }
        let param = self.u.symbol(sym).kind == Kind::TypeParam;
        if let Some(p) = prefix.filter(|_| !param && self.u.alias_of(sym).is_none()) {
            self.collect_parts(p, seen, parts);
        }
    }

    /// The types `t` is built from.
    fn children(&mut self, t: TypeId) -> Vec<TypeId> {
        let mut out = Vec::new();
        self.u.map_type(t, &mut |_, part| {
            (part != t).then(|| {
                out.push(part);
                part
            })
        });
        out
    }

    /// Whether `s` is a package, a package object or its class (dotty's `isExcluded`).
    fn is_excluded(&mut self, s: SymbolId) -> bool {
        let class = match self.u.is_module(s) {
            true => self.u.module_class(s),
            false => Some(s),
        };
        self.u.symbol(s).kind == Kind::Package || class.is_some_and(|c| self.is_named_package_object(c))
    }

    /// The companion object of class `c`, the object itself of a module class, if `c` is not
    /// nested in a class other than an object's.
    fn companion_module(&mut self, c: SymbolId) -> Option<SymbolId> {
        let owner = self.u.symbol(c).owner?;
        if self.u.is_class(owner) && !self.u.is_module_class(owner) {
            return None;
        }
        if self.u.is_module_class(c) {
            return self.u.source_module(c);
        }
        self.u.companion_module(c)
    }
}

/// The prefix of `t` if it refers to class `c` directly.
fn class_prefix(u: &tasty::Universe, t: TypeId, c: SymbolId) -> Option<TypeId> {
    match *u.types().get(t) {
        Type::Sym(p, s) if s == c => p,
        Type::Applied(tycon, _) => class_prefix(u, tycon, c),
        _ => None,
    }
}
