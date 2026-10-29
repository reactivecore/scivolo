// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! The implicits an implicit scope takes from the objects and values on the prefix paths of its
//! parts (dotty's `addPath`).

use super::super::Typer;
use super::candidates::Candidate;
use super::scope::ImplicitScope;
use tasty::{SymbolId, Type, TypeId};

/// How many bounds and intersections finding the classes of a path's type passes at most.
const MAX_BOUND_STEPS: u32 = 16;

impl Typer<'_> {
    /// Adds the implicit members of `p` to `out` if it is a static object other than a package
    /// object, or the `this` of one (dotty's `addPath` of a module).
    pub fn add_static_module(&mut self, p: TypeId, out: &mut ImplicitScope) {
        let s = match *self.u.types().get(p) {
            Type::This(c) => match self.u.class_symbol(c).filter(|&c| self.u.is_module_class(c)) {
                Some(c) => self.u.source_module(c),
                None => return,
            },
            _ => self.referenced(p),
        };
        if let Some(s) = s
            && self.u.is_module(s)
            && self.is_static_term(s)
            && !self.is_excluded(s)
            && let Some(class) = self.u.module_class(s)
        {
            let module_ref = crate::defn::type_ref(self.u, s);
            self.add_members(class, module_ref, out);
        }
    }

    /// Adds the implicit members of `class` selected from path `prefix`, and `prefix`, to `out`.
    pub fn add_members(&mut self, class: SymbolId, prefix: TypeId, out: &mut ImplicitScope) {
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

    /// Adds the implicit members of the stable values that are not static objects on path `p` to
    /// `out` (dotty's `addPath`).
    pub fn add_path(&mut self, p: TypeId, out: &mut ImplicitScope) -> Result<(), &'static str> {
        let outer = match *self.u.types().get(p) {
            Type::Sym(outer, _) => outer,
            Type::TermRef(outer, _) => Some(outer),
            _ => return Ok(()),
        };
        let s = self.referenced(p).ok_or("implicit scope through an unresolved path")?;
        if !self.u.symbol(s).kind.is_term()
            || self.is_excluded(s)
            || self.is_static_term(s) && self.u.is_module(s)
        {
            return Ok(());
        }
        self.complete(s);
        let info = self.u.info(s);
        if matches!(self.u.types().get(info), Type::Sym(_, x) if self.u.symbol(*x).kind.is_term()) {
            return self.add_path(info, out);
        }
        let widened = self.u.widen(p);
        let classes = self.bound_classes(widened, MAX_BOUND_STEPS);
        if classes.is_empty() {
            return Err("implicit scope through a value of no class type");
        }
        for class in classes {
            self.add_members(class, p, out);
        }
        match outer {
            Some(outer) => self.add_path(outer, out),
            None => Ok(()),
        }
    }

    /// The classes of `t`, through the upper bounds of abstract types and intersections.
    fn bound_classes(&mut self, t: TypeId, steps: u32) -> Vec<SymbolId> {
        if let Some(c) = self.u.class_symbol(t) {
            return vec![c];
        }
        let Some(steps) = steps.checked_sub(1) else { return Vec::new() };
        match self.u.types().get(t).clone() {
            Type::TypeRef(p, name) => {
                match self.u.refinement_named(p, &name).map(|i| self.u.types().get(i).clone()) {
                    Some(Type::Bounds(_, hi) | Type::Alias(hi)) => self.bound_classes(hi, steps),
                    _ => Vec::new(),
                }
            }
            Type::Refined(parent, ..) => self.bound_classes(parent, steps),
            Type::And(a, b) => {
                let mut classes = self.bound_classes(a, steps);
                classes.extend(self.bound_classes(b, steps));
                classes
            }
            Type::Sym(prefix, s) if self.u.is_type_symbol(s) => {
                let info = self.refined_info(prefix, s);
                let hi = match *self.u.types().get(info) {
                    Type::Bounds(_, hi) | Type::Alias(hi) => hi,
                    _ => return Vec::new(),
                };
                let info = match (prefix, self.u.symbol(s).owner) {
                    (Some(p), Some(o)) => self.u.as_seen_from(hi, p, o),
                    _ => hi,
                };
                self.bound_classes(info, steps)
            }
            Type::Sym(..) | Type::TermRef(..) | Type::Constant(..) | Type::Str(_) => {
                let widened = self.u.widen(t);
                if widened == t { Vec::new() } else { self.bound_classes(widened, steps) }
            }
            _ => Vec::new(),
        }
    }
}
