// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/PrepareInlineable.scala
//   compiler/src/dotty/tools/dotc/transform/AccessProxies.scala

mod accessor_defs;
mod accessor_types;
mod passing;
mod passing_defs;
mod retained;
mod rewire;

pub use retained::RETAINED_BODY;

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::NodeId;
use tasty::{SymbolId, Type};

/// Makes the body of one inline method inlineable (dotty's `PrepareInlineable.makeInlineable`):
/// the members it refers to that are not public where it expands, as `x` in
/// `class C { private var x = 0; inline def inc() = x += 1 }`, are reached through public
/// synthetic accessors of a class that sees them, `inline$x` and `inline$x_=`, or, if no class
/// around the method sees them, through accessors of the method's class that take the member's
/// receiver as their first parameter. Refuses private constructors, accessors in a class whose
/// body is typed already, members of generic base classes and accessed types it cannot rebuild.
pub struct MakeInlineable {
    /// The inline method whose body is made inlineable.
    meth: SymbolId,
    /// The context of the method's body.
    ctx: Ctx,
}

/// The inline accessors of the unit: made while the bodies of inline methods are typed, typed
/// and appended to their classes' bodies once those are typed.
#[derive(Default)]
pub struct Accessors {
    /// Each accessor made, by its class and what it accesses.
    made: FxHashMap<(SymbolId, Access), SymbolId>,
    /// The definitions of the accessors still to type, by class, with the context to type them in.
    pending: FxHashMap<SymbolId, Vec<(NodeId, Ctx)>>,
    /// The classes whose bodies are typed.
    closed: FxHashSet<SymbolId>,
}

/// The member an accessor reaches, and whether it assigns it.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Access {
    member: SymbolId,
    setter: bool,
}

impl Accessors {
    /// Whether `s` is one of the inline accessors made.
    fn is_accessor(&self, s: SymbolId) -> bool {
        self.made.values().any(|&a| a == s)
    }
}

impl Typer<'_> {
    /// Class body `body` of `cls`, typed in `ctx`, with the body retainers of its retained inline
    /// methods and its inline accessors (dotty's `inlineExpansion`, `addAccessorDefs`).
    pub fn with_inline_members(&mut self, cls: SymbolId, body: Vec<TNodeId>, ctx: Ctx) -> Vec<TNodeId> {
        let mut body = retained::with_retainers(self, body, ctx);
        for (def, def_ctx) in self.accessors.pending.remove(&cls).unwrap_or_default() {
            let def = self.typed_stat(def, def_ctx);
            accessor_types::with_type_trees(&mut self.tpd, def);
            passing_defs::with_setter_receiver(self, def);
            body.push(def);
        }
        self.accessors.closed.insert(cls);
        body
    }

    /// Replaces in `t`, in the body of inline method `meth`, the references that need inline
    /// accessors by references to them, and the assignments to such variables by setter calls.
    pub fn make_inlineable(&mut self, t: TNodeId, meth: SymbolId, ctx: Ctx) {
        MakeInlineable { meth, ctx }.run(self, t);
    }
}

impl MakeInlineable {
    /// Rewrites `tree` and its children, children first.
    fn run(&self, t: &mut Typer<'_>, tree: TNodeId) {
        if let T::Assign { lhs, .. } = *t.tpd.tree(tree)
            && let Some(var) = t.tpd.sym(lhs)
            && t.needs_inline_accessor_from(var, self.meth)
        {
            return self.assignment(t, tree, var);
        }
        if let Some(sym) = t.tpd.sym(tree)
            && t.needs_inline_accessor_from(sym, self.meth)
            && self.pass_receiver(t, tree, sym)
        {
            return self.children(t, tree);
        }

        let before = t.tpd.get(tree).tpe;
        let block_expr = match *t.tpd.tree(tree) {
            T::Block { expr, .. } => Some((expr, t.tpd.get(expr).tpe)),
            _ => None,
        };
        self.children(t, tree);
        if let Some((expr, old)) = block_expr
            && old == before
        {
            t.tpd.get_mut(tree).tpe = t.tpd.get(expr).tpe;
        }
        if let T::Select { qualifier, .. } = *t.tpd.tree(tree)
            && let Some(tpe) = before
            && let Type::Sym(Some(_), m) = *t.u.types().get(tpe)
            && t.tpd.sym(qualifier).is_some_and(|s| t.accessors.is_accessor(s))
        {
            let accessor = t.tpd.sym(qualifier).expect("an accessor");
            t.complete(accessor);
            let mut prefix = t.u.info(accessor);
            if let Type::ByName(r) = *t.u.types().get(prefix) {
                prefix = r;
            }
            t.tpd.get_mut(tree).tpe = Some(t.u.intern_type(Type::Sym(Some(prefix), m)));
        }
        if matches!(t.tpd.tree(tree), T::Ident { .. } | T::Select { .. })
            && let Some(sym) = t.tpd.sym(tree)
            && t.needs_inline_accessor_from(sym, self.meth)
        {
            self.rewire(t, tree, sym);
        }
    }

    /// Rewrites the children of `tree`.
    fn children(&self, t: &mut Typer<'_>, tree: TNodeId) {
        for c in t.tpd.children(tree) {
            self.run(t, c);
        }
    }
}
