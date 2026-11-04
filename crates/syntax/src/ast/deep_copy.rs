//! Copies of whole trees, every node under the root a new one.

use super::Ast;
use super::tree::{Clauses, List, NodeId, Tree};

impl Ast {
    /// A copy of `id` and of every tree under it, with their spans and attributes; modifiers
    /// stay shared.
    pub fn deep_copy(&mut self, id: NodeId) -> NodeId {
        let mut tree = self.tree(id).clone();
        match &mut tree {
            Tree::Select { qualifier: a, .. }
            | Tree::New { tpt: a }
            | Tree::NamedArg { arg: a, .. }
            | Tree::Quote { body: a }
            | Tree::SplicePattern { body: a }
            | Tree::Splice { expr: a }
            | Tree::Throw { expr: a }
            | Tree::MacroTree { expr: a }
            | Tree::SingletonTypeTree { reference: a }
            | Tree::ByNameTypeTree { result: a }
            | Tree::Bind { body: a, .. }
            | Tree::TypeDef { rhs: a, .. }
            | Tree::ModuleDef { imp: a, .. }
            | Tree::Parens { t: a }
            | Tree::ContextBoundTypeTree { tycon: a, .. }
            | Tree::UseRef { reference: a, .. } => *a = self.deep_copy(*a),
            Tree::This { qual: o } | Tree::Return { expr: o } => *o = self.deep_copy_opt(*o),
            Tree::Typed { expr: a, tpt: b }
            | Tree::Assign { lhs: a, rhs: b }
            | Tree::WhileDo { cond: a, body: b }
            | Tree::Annotated { arg: a, annot: b }
            | Tree::PostfixOp { od: a, op: b }
            | Tree::PrefixOp { op: a, od: b }
            | Tree::GenFrom { pat: a, expr: b, .. }
            | Tree::GenAlias { pat: a, expr: b } => {
                *a = self.deep_copy(*a);
                *b = self.deep_copy(*b);
            }
            Tree::Apply { fun: a, args: l }
            | Tree::TypeApply { fun: a, args: l }
            | Tree::AppliedTypeTree { tpt: a, args: l }
            | Tree::Import { expr: a, selectors: l }
            | Tree::PackageDef { pid: a, stats: l }
            | Tree::ContextBounds { bounds: a, cx_bounds: l }
            | Tree::CapturesAndResult { parent: a, refs: l }
            | Tree::LambdaTypeTree { tparams: l, body: a }
            | Tree::TermLambdaTypeTree { params: l, body: a }
            | Tree::PolyFunction { targs: l, body: a }
            | Tree::Function { args: l, body: a, .. }
            | Tree::ForYield { enums: l, expr: a }
            | Tree::ForDo { enums: l, body: a } => {
                *a = self.deep_copy(*a);
                *l = self.deep_copy_list(*l);
            }
            Tree::Alternative { trees: l }
            | Tree::Thicket { trees: l }
            | Tree::Tuple { trees: l }
            | Tree::InterpolatedString { segments: l, .. } => *l = self.deep_copy_list(*l),
            Tree::Super { qual: a, mix: o } | Tree::ValDef { tpt: a, rhs: o, .. } => {
                *a = self.deep_copy(*a);
                *o = self.deep_copy_opt(*o);
            }
            Tree::Block { stats: l, expr: o }
            | Tree::Match { cases: l, selector: o, .. }
            | Tree::RefinedTypeTree { refinements: l, tpt: o } => {
                *l = self.deep_copy_list(*l);
                *o = self.deep_copy_opt(*o);
            }
            Tree::If { cond: a, thenp: b, elsep: o, .. } | Tree::CaseDef { pat: a, body: b, guard: o } => {
                *a = self.deep_copy(*a);
                *b = self.deep_copy(*b);
                *o = self.deep_copy_opt(*o);
            }
            Tree::InfixOp { left: a, op: b, right: c } => {
                *a = self.deep_copy(*a);
                *b = self.deep_copy(*b);
                *c = self.deep_copy(*c);
            }
            Tree::Try { expr: a, cases: l, finalizer: o }
            | Tree::Export { expr: a, selectors: l, ext: o } => {
                *a = self.deep_copy(*a);
                *l = self.deep_copy_list(*l);
                *o = self.deep_copy_opt(*o);
            }
            Tree::ParsedTry { expr: a, handler: o, finalizer: p }
            | Tree::ImportSelector { imported: a, renamed: o, bound: p } => {
                *a = self.deep_copy(*a);
                *o = self.deep_copy_opt(*o);
                *p = self.deep_copy_opt(*p);
            }
            Tree::MatchTypeTree { bound: o, selector: a, cases: l }
            | Tree::PatDef { rhs: o, tpt: a, pats: l, .. } => {
                *o = self.deep_copy_opt(*o);
                *a = self.deep_copy(*a);
                *l = self.deep_copy_list(*l);
            }
            Tree::TypeBoundsTree { lo, hi, alias } => {
                *lo = self.deep_copy_opt(*lo);
                *hi = self.deep_copy_opt(*hi);
                *alias = self.deep_copy_opt(*alias);
            }
            Tree::DefDef { paramss, tpt, rhs, .. } => {
                *paramss = self.deep_copy_clauses(*paramss);
                *tpt = self.deep_copy(*tpt);
                *rhs = self.deep_copy_opt(*rhs);
            }
            Tree::ExtMethods { paramss, methods } => {
                *paramss = self.deep_copy_clauses(*paramss);
                *methods = self.deep_copy_list(*methods);
            }
            Tree::Template { constr, parents_or_derived, self_, body, .. } => {
                *constr = self.deep_copy(*constr);
                *parents_or_derived = self.deep_copy_list(*parents_or_derived);
                *self_ = self.deep_copy_opt(*self_);
                *body = self.deep_copy_list(*body);
            }
            Tree::Ident { .. }
            | Tree::Literal { .. }
            | Tree::TypeTree
            | Tree::SymbolLit { .. }
            | Tree::Number { .. }
            | Tree::TypedSplice { .. }
            | Tree::DerivedTypeTree { .. }
            | Tree::Empty => {}
        }
        let copy = self.copy_node(id);
        self.get_mut(copy).tree = tree;
        copy
    }

    fn deep_copy_opt(&mut self, o: Option<NodeId>) -> Option<NodeId> {
        o.map(|t| self.deep_copy(t))
    }

    fn deep_copy_list(&mut self, l: List) -> List {
        let items: Vec<NodeId> = self.items(l).to_vec().into_iter().map(|t| self.deep_copy(t)).collect();
        self.list(&items)
    }

    fn deep_copy_clauses(&mut self, c: Clauses) -> Clauses {
        let lists: Vec<List> =
            self.clause_items(c).to_vec().into_iter().map(|l| self.deep_copy_list(l)).collect();
        self.clauses(&lists)
    }
}
