// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala

mod alternatives;
mod applicability;
mod applies;
mod compatible;
mod errors;
mod mapped;
mod named;
mod parameterless;
mod poly;
mod poly_instances;
mod pretyped;
mod resolve;
mod shapes;
mod values;

use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use crate::typer::{Pt, Typer};
use syntax::ast::NodeId;
use tasty::{SymbolId, Type, TypeId};

/// Why alternatives are compared undecided: an implicit conversion outside the subset.
const UNDECIDED: &str = "overload resolution with implicit conversions";
/// Why a value with `apply` methods among the alternatives is not resolved.
const VALUE_APPLY: &str = "overloaded value with an apply method";
/// Why a function argument with modifiers is not resolved against the alternatives.
const MODIFIED_FUNCTION: &str = "function argument with modifiers of an overload";

/// Chooses the alternative an overloaded name denotes by what the context expects of it (dotty's
/// `resolveOverloaded`). Applied to arguments, as `f(1)` with `def f(x: Int)` and
/// `def f(x: Any)`, it takes the most specific alternative they apply to, by implicit conversions
/// only if none applies without; applied to type arguments, as `f[Int]`, those taking as many;
/// not applied, as in `val g: Int => Int = f`, the one fitting the expected type. Its methods take
/// the typer: arguments are typed while alternatives are compared, and comparing completes their
/// symbols. Resolving copies of the alternatives, as by a later argument clause in `f(1)(2)`, is
/// a nested resolution.
#[derive(Clone, Copy)]
pub struct Resolution {
    /// The type the alternatives are members of; none for local definitions and copies.
    prefix: Option<TypeId>,
    /// What the context expects of the reference.
    pt: Pt,
}

/// Why an overloaded name denotes no alternative: the construct outside the subset or the error
/// dotty reports.
pub struct Unchosen {
    pub what: &'static str,
    /// The alternatives that apply equally well, if the application is ambiguous.
    pub ambiguous: Option<Vec<SymbolId>>,
}

impl From<&'static str> for Unchosen {
    fn from(what: &'static str) -> Self {
        Unchosen { what, ambiguous: None }
    }
}

impl Typer<'_> {
    /// Chooses the alternative of `syms`, members of `prefix`, that `pt` selects: the only one,
    /// the only method taking as many arguments as `pt` supplies, or the one overload resolution
    /// picks.
    pub fn choose(
        &mut self,
        syms: &[SymbolId],
        prefix: Option<TypeId>,
        pt: Pt,
    ) -> Result<SymbolId, Unchosen> {
        Resolution { prefix, pt }.choose(self, syms)
    }

    /// Checks whether an argument of type `arg` fits a parameter of type `param` in `ctx`,
    /// through an implicit conversion if `views` (dotty's `isCompatible`, else its direct
    /// applicability); `None` if undecided.
    pub fn compatible(&mut self, arg: TypeId, param: TypeId, views: bool, ctx: Ctx) -> Option<bool> {
        compatible::compatible(self, arg, param, views, ctx)
    }

    /// Checks whether arguments of types `args` fit the first parameter clause of `alt` seen
    /// from `prefix` in `ctx`, through implicit conversions if `views`, as `f(1, "a")` fits
    /// `def f(x: Long, y: String)`; `None` if undecided.
    pub fn applicable(
        &mut self,
        alt: SymbolId,
        prefix: Option<TypeId>,
        args: &[TypeId],
        views: bool,
        ctx: Ctx,
    ) -> Option<bool> {
        applicability::applicable(self, alt, prefix, args, views, ctx)
    }

    /// Finds the parameter types of the first clause of `alt` seen from `prefix`, a repeated one
    /// as its element type, and whether the last is repeated; of a polymorphic `alt` only if
    /// `poly`, its type parameters replaced by fresh type variables within their bounds.
    pub fn param_types(
        &mut self,
        alt: SymbolId,
        prefix: Option<TypeId>,
        poly: bool,
    ) -> Option<(Vec<TypeId>, bool)> {
        applicability::param_types(self, alt, prefix, poly)
    }

    /// Instantiates polymorphic type `info` of `owner` with fresh types: its type parameters
    /// replaced by abstract types within their bounds if `rigid`, else by type variables within
    /// them, or by the type arguments an overloaded `f[Int]` is resolved with (dotty's
    /// `constrained`, `newTypeParams`); `info` itself if not polymorphic.
    pub fn poly_instance(&mut self, info: TypeId, owner: SymbolId, rigid: bool) -> TypeId {
        poly_instances::poly_instance(self, info, owner, rigid)
    }

    /// Strips the leading implicit clauses off type `info` of method `meth`, references to their
    /// parameters made wildcards bounded by their types, and counts the clauses stripped (dotty's
    /// `stripImplicit` with `resultTypeApprox`).
    pub fn strip_implicit_clauses(&mut self, info: TypeId, meth: SymbolId) -> (TypeId, usize) {
        mapped::strip_implicit_clauses(self.u, info, meth)
    }

    /// Types argument `a` of the chosen alternative against its parameter type `formal`:
    /// `pretyped` adapted if overload resolution typed it before, else typed now.
    pub fn arg_against(&mut self, a: NodeId, pretyped: Option<TNodeId>, formal: TypeId, ctx: Ctx) -> TNodeId {
        match pretyped {
            Some(typed) => {
                let arg = self.to_sam(typed, formal).unwrap_or(typed);
                self.adapt(arg, Pt::Type(formal), ctx)
            }
            None => self.typed(a, Pt::Type(formal), ctx),
        }
    }

    /// Reports `NoMatchingOverload` for identifier `id` of methods `alts`, members of `prefix`,
    /// applied to the arguments of `pt`, or `AmbiguousOverload` if `ambiguous` of them match;
    /// `None` where dotty may word or decide it otherwise.
    pub fn overload_error(
        &mut self,
        id: NodeId,
        (alts, prefix): (&[SymbolId], Option<TypeId>),
        pt: Pt,
        ambiguous: Option<Vec<SymbolId>>,
    ) -> Option<TNodeId> {
        Resolution { prefix, pt }.overload_error(self, id, alts, ambiguous)
    }

    /// Gives the tree for reference `id` when no alternative is chosen for `what`: an error tree
    /// when an argument of `pt` has a reported error, else `what` unsupported.
    pub fn unchosen(&mut self, id: NodeId, what: &str, pt: Pt) -> TNodeId {
        if self.unusable_for_inference(pt) {
            return self.error_tree(self.ast.span(id));
        }
        self.unsupported(id, what)
    }

    /// Checks whether a typed argument of `pt` has the error type in it and that error is
    /// reported (dotty's `unusableForInference`), as in `f(undefined)`.
    pub fn unusable_for_inference(&mut self, pt: Pt) -> bool {
        let Pt::Fun(proto) = pt else { return false };
        let args = self.protos[proto].typed.clone().unwrap_or_default();
        let reported = self.diags.has_errors() || self.unsupported.is_some();
        reported
            && args.into_iter().flatten().any(|a| {
                let tpe = self.tpd.tpe(a);
                let tpe = self.zonk(tpe);
                self.type_parts(tpe).into_iter().any(|p| *self.u.types().get(p) == Type::Error)
            })
    }
}
