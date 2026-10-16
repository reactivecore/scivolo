//! The positioned children of each tree, in the order of dotty's case class fields.

use super::Ast;
use super::tree::{Clauses, List, ModsId, NodeId, Tree};

#[derive(Clone, Copy, Debug)]
pub enum Child {
    Node(NodeId),
    /// An absent tree (`EmptyTree`).
    Empty,
    List(List),
    Clauses(Clauses),
    /// Modifiers that are a field of the tree (only `PatDef`'s).
    Mods(ModsId),
}

/// The at most four children of a tree.
pub struct Children {
    items: [Child; 4],
    len: usize,
}

impl Children {
    fn of(cs: &[Child]) -> Children {
        let mut items = [Child::Empty; 4];
        items[..cs.len()].copy_from_slice(cs);
        Children { items, len: cs.len() }
    }
}

impl std::ops::Deref for Children {
    type Target = [Child];

    fn deref(&self) -> &[Child] {
        &self.items[..self.len]
    }
}

impl Ast {
    /// The trees under `id` in the positioned children, `id` itself included.
    fn for_each_tree(&self, id: NodeId, f: &mut dyn FnMut(NodeId) -> bool) {
        if !f(id) {
            return;
        }
        for n in self.child_nodes(id) {
            self.for_each_tree(n, f);
        }
    }

    /// The trees of the positioned children of `id`, in order.
    pub fn child_nodes(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        for &c in self.children(id).iter() {
            match c {
                Child::Node(n) => out.push(n),
                Child::List(l) => out.extend_from_slice(self.items(l)),
                Child::Clauses(cs) => {
                    self.clause_items(cs).iter().for_each(|&l| out.extend_from_slice(self.items(l)))
                }
                Child::Empty | Child::Mods(_) => {}
            }
        }
        out
    }

    /// Whether `id` or a tree under it satisfies `p` (dotty's `existsSubTree`).
    pub fn exists_subtree(&self, id: NodeId, p: &dyn Fn(&Ast, NodeId) -> bool) -> bool {
        let mut found = false;
        self.for_each_tree(id, &mut |n| {
            found = found || p(self, n);
            !found
        });
        found
    }

    /// The outermost trees at or under `id` that satisfy `p`, in order.
    pub fn subtrees(&self, id: NodeId, p: &dyn Fn(&Ast, NodeId) -> bool) -> Vec<NodeId> {
        let mut out = Vec::new();
        self.for_each_tree(id, &mut |n| {
            let hit = p(self, n);
            if hit {
                out.push(n);
            }
            !hit
        });
        out
    }

    pub fn children(&self, id: NodeId) -> Children {
        self.children_of(self.tree(id))
    }

    /// The tree fields of `tree` in declaration order.
    pub fn children_of(&self, tree: &Tree) -> Children {
        use Child::{List as L, Node as N};
        let o = |t: &Option<NodeId>| t.map_or(Child::Empty, N);
        let c = Children::of;
        match tree {
            Tree::Ident { .. }
            | Tree::Literal { .. }
            | Tree::TypeTree
            | Tree::SymbolLit { .. }
            | Tree::Number { .. }
            | Tree::TypedSplice { .. }
            | Tree::DerivedTypeTree { .. }
            | Tree::Empty => c(&[]),
            Tree::Select { qualifier, .. } => c(&[N(*qualifier)]),
            Tree::This { qual } => c(&[o(qual)]),
            Tree::Super { qual, mix } => c(&[N(*qual), o(mix)]),
            Tree::Apply { fun, args } | Tree::TypeApply { fun, args } => c(&[N(*fun), L(*args)]),
            Tree::New { tpt } => c(&[N(*tpt)]),
            Tree::Typed { expr, tpt } => c(&[N(*expr), N(*tpt)]),
            Tree::NamedArg { arg, .. } => c(&[N(*arg)]),
            Tree::Assign { lhs, rhs } => c(&[N(*lhs), N(*rhs)]),
            Tree::Block { stats, expr } => c(&[L(*stats), o(expr)]),
            Tree::If { cond, thenp, elsep, .. } => c(&[N(*cond), N(*thenp), o(elsep)]),
            Tree::Match { selector, cases, .. } => c(&[o(selector), L(*cases)]),
            Tree::CaseDef { pat, guard, body } => c(&[N(*pat), o(guard), N(*body)]),
            Tree::Return { expr } => c(&[o(expr)]),
            Tree::WhileDo { cond, body } => c(&[N(*cond), N(*body)]),
            Tree::Quote { body } | Tree::SplicePattern { body } => c(&[N(*body)]),
            Tree::Splice { expr } | Tree::Throw { expr } | Tree::MacroTree { expr } => c(&[N(*expr)]),
            Tree::SingletonTypeTree { reference } => c(&[N(*reference)]),
            Tree::RefinedTypeTree { tpt, refinements } => c(&[o(tpt), L(*refinements)]),
            Tree::AppliedTypeTree { tpt, args } => c(&[N(*tpt), L(*args)]),
            Tree::LambdaTypeTree { tparams: ps, body }
            | Tree::TermLambdaTypeTree { params: ps, body }
            | Tree::PolyFunction { targs: ps, body } => c(&[L(*ps), N(*body)]),
            Tree::Function { args, body, .. } => c(&[L(*args), N(*body)]),
            Tree::MatchTypeTree { bound, selector, cases } => c(&[o(bound), N(*selector), L(*cases)]),
            Tree::ByNameTypeTree { result } => c(&[N(*result)]),
            Tree::TypeBoundsTree { lo, hi, alias } => c(&[o(lo), o(hi), o(alias)]),
            Tree::Bind { body, .. } => c(&[N(*body)]),
            Tree::Alternative { trees } | Tree::Thicket { trees } | Tree::Tuple { trees } => c(&[L(*trees)]),
            Tree::ValDef { tpt, rhs, .. } => c(&[N(*tpt), o(rhs)]),
            Tree::DefDef { paramss, tpt, rhs, .. } => c(&[Child::Clauses(*paramss), N(*tpt), o(rhs)]),
            Tree::TypeDef { rhs, .. } => c(&[N(*rhs)]),
            Tree::Template { constr, parents_or_derived, self_, body, .. } => {
                c(&[N(*constr), L(*parents_or_derived), o(self_), L(*body)])
            }
            Tree::Import { expr, selectors } | Tree::Export { expr, selectors, .. } => {
                c(&[N(*expr), L(*selectors)])
            }
            Tree::PackageDef { pid, stats } => c(&[N(*pid), L(*stats)]),
            Tree::Annotated { arg, annot } => c(&[N(*arg), N(*annot)]),
            Tree::ModuleDef { imp, .. } => c(&[N(*imp)]),
            Tree::ParsedTry { expr, handler, finalizer } => c(&[N(*expr), o(handler), o(finalizer)]),
            Tree::Try { expr, cases, finalizer } => c(&[N(*expr), L(*cases), o(finalizer)]),
            Tree::InterpolatedString { segments, .. } => c(&[L(*segments)]),
            Tree::InfixOp { left, op, right } => c(&[N(*left), N(*op), N(*right)]),
            Tree::PostfixOp { od, op } => c(&[N(*od), N(*op)]),
            Tree::PrefixOp { op, od } => c(&[N(*op), N(*od)]),
            Tree::Parens { t } => c(&[N(*t)]),
            Tree::ForYield { enums, expr: body } | Tree::ForDo { enums, body } => c(&[L(*enums), N(*body)]),
            Tree::GenFrom { pat, expr, .. } | Tree::GenAlias { pat, expr } => c(&[N(*pat), N(*expr)]),
            Tree::ContextBounds { bounds, cx_bounds } => c(&[N(*bounds), L(*cx_bounds)]),
            Tree::PatDef { mods, pats, tpt, rhs } => c(&[Child::Mods(*mods), L(*pats), N(*tpt), o(rhs)]),
            Tree::ExtMethods { paramss, methods } => c(&[Child::Clauses(*paramss), L(*methods)]),
            Tree::ContextBoundTypeTree { tycon, .. } => c(&[N(*tycon)]),
            Tree::ImportSelector { imported, renamed, bound } => c(&[N(*imported), o(renamed), o(bound)]),
            Tree::CapturesAndResult { refs, parent } => c(&[L(*refs), N(*parent)]),
            Tree::UseRef { reference, .. } => c(&[N(*reference)]),
        }
    }
}
