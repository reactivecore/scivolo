// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Deriving.scala

mod finalize;
mod instance;
mod kinds;
mod trees;

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashMap;
use syntax::ast::{NodeId, Tree};
use tasty::SymbolId;

/// The type class instances of one class's `derives` clause (dotty's `Deriver`): for
/// `case class C(x: Int) derives Eq`, the definition `given derived$Eq: Eq[C] = Eq.derived` in
/// the companion of `C`, entered with the companion and typed after the statements around it.
/// Handles type classes of one parameter of any kind, as `Functor` for `C[A]` through
/// `[X] =>> C[X]`, and `CanEqual`; refuses type classes whose companion has no stable path and
/// classes whose type parameter bounds depend on each other.
pub struct Deriver {
    /// The class the instances are for: the companion class of the module class that holds
    /// them, or the module class itself for `object O derives TC`.
    cls: SymbolId,
    /// The context of the module class's body.
    ctx: Ctx,
    /// The names of the instances made so far, `derived$TC` for each.
    names: Vec<String>,
}

/// The instances of `derives` clauses entered and not typed yet, from the indexing of the
/// classes that hold them until the statements around them are typed.
#[derive(Default)]
pub struct Derivations {
    /// The instances of each module class's `derives` clause.
    instances: FxHashMap<SymbolId, Vec<DerivedInstance>>,
    /// The type lambdas of derived instances' types, by the instances that own their parameters.
    lambdas: FxHashMap<NodeId, NodeId>,
}

/// The untyped definition of a derived instance, and why its `TC.derived` cannot be called if it
/// cannot.
pub struct DerivedInstance {
    def: NodeId,
    problem: Option<Problem>,
}

/// Why an instance's `TC.derived` cannot be called: the type class, or its companion's path.
enum Problem {
    NoCompanion(String),
    ExplicitParams(String),
}

impl Derivations {
    /// Finds the derived instance whose type parameters type lambda tree `lambda` binds, as the
    /// `[X] =>> C[X]` in `given derived$Functor: Functor[[X] =>> C[X]]`.
    pub fn lambda_owner(&self, lambda: NodeId) -> Option<NodeId> {
        self.lambdas.get(&lambda).copied()
    }
}

impl Typer<'_> {
    /// Enters into module class `cls` the instances of its `derives` clause `derived`, for its
    /// companion class if `companion`, else for itself (dotty's `Deriver.enterDerived`).
    pub fn enter_derived(&mut self, cls: SymbolId, companion: bool, derived: &[NodeId], ctx: Ctx) {
        let deriving = if companion { self.u.companion_class(cls).unwrap_or(cls) } else { cls };
        let mut deriver = Deriver { cls: deriving, ctx, names: Vec::new() };
        let mut instances = Vec::new();
        for &d in derived {
            if matches!(self.ast.tree(d), Tree::UseRef { .. }) {
                self.unsupported(d, "uses clause");
                continue;
            }
            instances.extend(deriver.instance(self, d));
        }

        let nodes: Vec<NodeId> = instances.iter().map(|i| i.def).collect();
        self.index(&nodes, ctx);
        self.derivations.instances.insert(cls, instances);
    }

    /// Appends to the body of each module class among typed statements `stats`, typed in `ctxs`,
    /// the instances of its `derives` clause (dotty's `Deriver.finalize`).
    pub fn finalize_derived(&mut self, stats: &[TNodeId], ctxs: &[Ctx]) {
        for (&s, &ctx) in stats.iter().zip(ctxs) {
            let T::TypeDef { rhs, .. } = *self.tpd.tree(s) else { continue };
            let Some(cls) = self.tpd.sym(s).filter(|c| self.derivations.instances.contains_key(c)) else {
                continue;
            };
            let cctx = self.class_ctx(cls, ctx);
            let derived = finalize::typed_instances(self, cls, cctx);
            let T::Template { constr, parents, self_, body } = *self.tpd.tree(rhs) else { continue };
            let body = [self.tpd.items(body), &derived].concat();
            let body = self.tpd.list(&body);
            self.tpd.get_mut(rhs).tree = T::Template { constr, parents, self_, body };
        }
    }
}
