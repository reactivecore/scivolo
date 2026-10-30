// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! Type trees of `Printer.TreeCode` (dotty's `printTypeTree`).

use super::{Code, REFUSED, Shown};
use tasty::{Type, TypeId};
use typer::tpd::{TNodeId, Tree as T};

impl Code<'_> {
    /// Whether `t` is a type tree; `Wildcard()` is not.
    pub fn is_type_tree(&self, t: TNodeId) -> bool {
        let is_type = self.h.tpd.sym(t).is_some_and(|s| self.h.u.symbol(s).kind.is_type());
        match *self.h.tpd.tree(t) {
            T::Ident { name } => is_type && !(!name.is_type && self.name(name) == "_"),
            T::Select { .. } => is_type,
            T::TypeTree { .. }
            | T::SingletonTypeTree { .. }
            | T::ByNameTypeTree { .. }
            | T::AppliedTypeTree { .. }
            | T::LambdaTypeTree { .. }
            | T::RefinedTypeTree { .. }
            | T::TypeBoundsTree { .. }
            | T::Annotated { .. } => true,
            _ => false,
        }
    }

    pub fn type_tree(&mut self, t: TNodeId) -> Shown {
        let t = self.r(t);
        let tpe = self.h.tpd.get(t).tpe;
        match self.tree_of(t) {
            T::TypeTree { .. } => {
                let tpe = tpe.ok_or(REFUSED)?;
                match *self.h.u.types().get(tpe) {
                    Type::Bounds(lo, hi) => {
                        self.push("_ >: ");
                        self.ty(lo)?;
                        self.push(" <: ");
                        return self.ty(hi);
                    }
                    Type::Annotated(..) => return Err(REFUSED),
                    _ => {}
                }
                self.ty(tpe)?;
                if self.is_singleton(tpe) {
                    self.push(".type");
                }
            }
            T::Ident { .. } => self.ty(tpe.ok_or(REFUSED)?)?,
            T::Select { qualifier, name } => {
                self.tree(qualifier)?;
                self.push(&format!(".{}", self.name(name)));
            }
            T::SingletonTypeTree { reference } => {
                self.tree(reference)?;
                if !matches!(self.tree_of(reference), T::Literal { .. }) {
                    self.push(".type");
                }
            }
            T::AppliedTypeTree { tpt, args } => {
                self.type_tree(tpt)?;
                self.push("[");
                self.trees(&self.items(args), ", ")?;
                self.push("]");
            }
            T::Annotated { arg, annot } => {
                let class = self.h.tpd.get(self.r(annot)).tpe.and_then(|a| self.h.u.class_symbol(a));
                if class.is_none_or(|c| self.h.u.full_name(c) != "scala.annotation.internal.Repeated") {
                    return Err(REFUSED);
                }
                let seq = self.h.tpd.tpe(self.r(arg));
                let Type::Applied(_, args) = self.h.u.types().get(seq).clone() else { return Err(REFUSED) };
                let [elem] = args[..] else { return Err(REFUSED) };
                self.ty(elem)?;
                self.push("*");
            }
            T::ByNameTypeTree { result } => {
                self.push("=> ");
                self.type_tree(result)?;
            }
            T::TypeBoundsTree { lo, hi, alias: None } => {
                self.push("_ >: ");
                self.type_tree(lo)?;
                self.push(" <: ");
                self.type_tree(hi)?;
            }
            _ => return Err(REFUSED),
        }
        Ok(())
    }

    /// Whether an inferred type tree of `t` prints with `.type`: a term reference or a module
    /// class.
    fn is_singleton(&mut self, t: TypeId) -> bool {
        match *self.h.u.types().get(t) {
            Type::Sym(_, s) => self.h.u.symbol(s).kind.is_term() || self.h.u.is_module_class(s),
            Type::TermRef(..) => true,
            Type::TypeRef(..) => self.h.u.resolve_ref(t).is_some_and(|s| self.h.u.is_module_class(s)),
            _ => false,
        }
    }

    /// The right-hand side of a type parameter's definition: a bounds tree, a type tree of
    /// bounds, or another type.
    pub fn type_def_rhs(&mut self, rhs: TNodeId) -> Shown {
        let rhs = self.r(rhs);
        if let T::TypeBoundsTree { lo, hi, alias: None } = self.tree_of(rhs) {
            return self.bounds_tree(Bound::Tree(lo), Bound::Tree(hi));
        }
        let tpe = self.h.tpd.tpe(rhs);
        match *self.h.u.types().get(tpe) {
            Type::Bounds(lo, hi) if matches!(self.tree_of(rhs), T::TypeTree { .. }) => {
                self.bounds_tree(Bound::Inferred(lo), Bound::Inferred(hi))
            }
            _ => self.ty(tpe),
        }
    }

    /// The bounds of a type parameter (dotty's `printBoundsTree`).
    pub fn bounds_tree(&mut self, lo: Bound, hi: Bound) -> Shown {
        let (lo_t, hi_t) = (self.bound_type(lo), self.bound_type(hi));
        if lo_t == hi_t {
            self.push(" = ");
            return self.bound(lo);
        }
        let builtins = self.h.u.builtins();
        let inferred = |c: &Self, b: Bound| match b {
            Bound::Tree(t) => matches!(c.tree_of(t), T::TypeTree { .. }),
            Bound::Inferred(_) => true,
        };
        if !(inferred(self, lo) && self.h.u.class_symbol(lo_t) == Some(builtins.nothing)) {
            self.push(" >: ");
            self.bound(lo)?;
        }
        if !(inferred(self, hi) && self.h.u.class_symbol(hi_t) == Some(builtins.any)) {
            self.push(" <: ");
            self.bound(hi)?;
        }
        Ok(())
    }

    fn bound_type(&self, b: Bound) -> TypeId {
        match b {
            Bound::Tree(t) => self.h.tpd.tpe(self.r(t)),
            Bound::Inferred(t) => t,
        }
    }

    fn bound(&mut self, b: Bound) -> Shown {
        match b {
            Bound::Tree(t) => self.type_tree(t),
            Bound::Inferred(t) => self.ty(t),
        }
    }
}

/// A bound of a bounds tree: a tree, inferred if a `TypeTree`, or an inferred type.
#[derive(Clone, Copy)]
pub enum Bound {
    Tree(TNodeId),
    Inferred(TypeId),
}
