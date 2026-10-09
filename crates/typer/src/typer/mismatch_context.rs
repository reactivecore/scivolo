//! Whether dotty types a tree with the expected type the typer adapts it to, and reports a
//! mismatch there with no retry: the trees from the unit's root down to it are definitions,
//! right-hand sides, blocks, branches, closure bodies, local assignments, or arguments of a method
//! that is neither overloaded nor polymorphic and whose qualifier no view may convert.

use super::Typer;
use crate::ctx::Ctx;
use syntax::ast::{Ast, List, NodeId, Span, Tree};
use tasty::tags::{MUTABLE, OBJECT};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether the tree at `span`, typed in `ctx`, is where dotty types it with the same expected
    /// type and reports a mismatch; the trees at `span` lie on one path.
    pub fn certain_context(&mut self, span: Span, ctx: Ctx) -> bool {
        let mut path = Vec::new();
        self.u.symbol(ctx.owner).name != "<init>"
            && node_path(self.ast, self.unit, span, &mut path)
            && path.iter().filter(|&&n| has_span(self.ast, n, span)).count()
                == count_spans(self.ast, self.unit, span)
            && path.windows(2).all(|w| self.certain_edge(w[0], w[1], ctx))
    }

    /// Whether dotty types child `child` of `parent` with the expected type the typer does.
    fn certain_edge(&mut self, parent: NodeId, child: NodeId, ctx: Ctx) -> bool {
        let in_list = |t: &Self, l: List| t.ast.items(l).contains(&child);
        match self.ast.tree(parent).clone() {
            Tree::PackageDef { stats, .. } | Tree::Block { stats, expr: None } => in_list(self, stats),
            Tree::Template { body, .. } => in_list(self, body),
            Tree::ModuleDef { imp, .. } => imp == child,
            Tree::TypeDef { rhs, .. } => rhs == child && matches!(self.ast.tree(rhs), Tree::Template { .. }),
            Tree::ValDef { rhs, .. } | Tree::DefDef { rhs, .. } => rhs == Some(child),
            Tree::Block { .. } | Tree::Thicket { .. } | Tree::Parens { .. } | Tree::Return { .. } => true,
            Tree::If { .. } => true,
            Tree::Match { cases, .. } => in_list(self, cases),
            Tree::CaseDef { body, .. } => body == child,
            Tree::Try { expr, cases, .. } => expr == child || in_list(self, cases),
            Tree::ParsedTry { expr, handler, .. } => {
                expr == child
                    || handler == Some(child)
                        && matches!(self.ast.tree(child), Tree::Match { selector: None, .. })
            }
            Tree::Function { body, .. } => body == child,
            Tree::Typed { expr, tpt } => {
                let plain = match *self.ast.tree(tpt) {
                    Tree::Ident { name } => self.text(name.name) != "_*",
                    Tree::Annotated { .. } => false,
                    _ => true,
                };
                expr == child && plain
            }
            Tree::Assign { lhs, rhs } => rhs == child && self.is_local_var(lhs, ctx),
            Tree::Apply { fun, args } => {
                let plain =
                    |a: &NodeId| !matches!(self.ast.tree(*a), Tree::NamedArg { .. } | Tree::Typed { .. });
                in_list(self, args)
                    && self.ast.items(args).iter().all(plain)
                    && self.plain_callee(fun, args, ctx)
            }
            _ => false,
        }
    }

    /// Whether `lhs` is a local variable.
    fn is_local_var(&mut self, lhs: NodeId, ctx: Ctx) -> bool {
        let Tree::Ident { name } = *self.ast.tree(lhs) else { return false };
        let text = self.text(name.name).to_owned();
        let Some(s) = self.only_ref(&text, false, ctx) else { return false };
        let sym = self.u.symbol(s);
        let local = sym.owner.is_some_and(|o| self.u.symbol(o).kind.is_term());
        sym.kind == Kind::Val && sym.flags.has(MUTABLE) && local && self.type_is_settled(s)
    }

    /// Whether `fun` applied to `args` calls a method of one parameter list that dotty finds as
    /// the only alternative and no view on its qualifier may replace: a method, an object's
    /// method or `apply`, or a class's constructor.
    fn plain_callee(&mut self, fun: NodeId, args: List, ctx: Ctx) -> bool {
        let arity = self.ast.items(args).len();
        let (owner, name) = match *self.ast.tree(fun) {
            Tree::Ident { name } => {
                let text = self.text(name.name).to_owned();
                if self.defined_in(args, &text) {
                    return false;
                }
                let Some(s) = self.only_ref(&text, false, ctx) else { return false };
                if self.u.symbol(s).kind == Kind::Def {
                    return self.is_monomorphic(s, arity);
                }
                (s, "apply".to_owned())
            }
            Tree::Select { qualifier, name } => {
                let text = self.text(name.name).to_owned();
                match *self.ast.tree(qualifier) {
                    Tree::Ident { name: q } => {
                        let q = self.text(q.name).to_owned();
                        if self.defined_in(args, &q) {
                            return false;
                        }
                        let Some(s) = self.only_ref(&q, false, ctx) else { return false };
                        (s, text)
                    }
                    Tree::New { tpt } if text == "<init>" => return self.plain_constructor(tpt, arity, ctx),
                    _ => return false,
                }
            }
            _ => return false,
        };
        if !self.u.symbol(owner).flags.has(OBJECT) {
            return false;
        }
        let Some(class) = self.u.module_class(owner) else { return false };
        let site = crate::defn::type_ref(self.u, owner);
        match self.class_members(class, &name)[..] {
            [m] => self.is_monomorphic(m, arity) && !self.may_supply((class, site), &name, ctx),
            _ => false,
        }
    }

    /// Whether `new C(..)` with `arity` arguments calls the only constructor of a class without
    /// type parameters.
    fn plain_constructor(&mut self, tpt: NodeId, arity: usize, ctx: Ctx) -> bool {
        let Tree::Ident { name } = *self.ast.tree(tpt) else { return false };
        let text = self.text(name.name).to_owned();
        let Some(c) = self.only_ref(&text, true, ctx) else { return false };
        if self.u.symbol(c).kind != Kind::Class
            || !self.u.type_params(c).is_empty()
            || !self.is_concrete_class(c)
        {
            return false;
        }
        matches!(self.class_members(c, "<init>")[..], [init] if self.is_monomorphic(init, arity))
    }

    /// The only symbol `name` refers to in `ctx`, a term the class it is found in has no other
    /// member of that name.
    fn only_ref(&mut self, name: &str, is_type: bool, ctx: Ctx) -> Option<SymbolId> {
        let Ok(Some(found)) = self.find_ref(name, is_type, ctx) else { return None };
        let [s] = found.syms[..] else { return None };
        let owner = self.u.symbol(s).owner?;
        let kind = self.u.symbol(owner).kind;
        if is_type || kind == Kind::Package || kind.is_term() {
            return Some(s);
        }
        let class = match found.prefix {
            Some(p) if !matches!(self.u.types().get(p), Type::Package(_)) => {
                let widened = self.u.widen(p);
                self.u.class_symbol(widened)?
            }
            _ => Some(owner).filter(|&o| self.u.symbol(o).kind == Kind::Class)?,
        };
        (self.class_members(class, name).len() == 1).then_some(s)
    }

    /// Whether method `m` takes `arity` parameters, none repeated, in one explicit parameter list
    /// and no type parameters.
    fn is_monomorphic(&mut self, m: SymbolId, arity: usize) -> bool {
        self.complete(m);
        let info = self.u.info(m);
        let Type::Method { params, result, implicit: false, .. } = self.u.types().get(info).clone() else {
            return false;
        };
        let params: Vec<TypeId> = params.into_iter().map(|(_, p)| p).collect();
        let curried = matches!(self.u.types().get(result), Type::Method { .. } | Type::Poly { .. });
        params.len() == arity && !curried && params.into_iter().all(|p| self.repeated_elem(p).is_none())
    }

    /// Whether a definition or parameter under the trees of `args` is named `name`.
    fn defined_in(&self, args: List, name: &str) -> bool {
        let defines = |ast: &Ast, n: NodeId| match *ast.tree(n) {
            Tree::ValDef { name: d, .. }
            | Tree::DefDef { name: d, .. }
            | Tree::Bind { name: d, .. }
            | Tree::ModuleDef { name: d, .. } => self.text(d.name) == name,
            _ => false,
        };
        self.ast.items(args).iter().any(|&a| self.ast.exists_subtree(a, &defines))
    }
}

/// Whether a tree from the start to the end of `span` is `from` or under it; the trees from `from`
/// to the innermost such tree pushed onto `path` if so.
fn node_path(ast: &Ast, from: NodeId, span: Span, path: &mut Vec<NodeId>) -> bool {
    path.push(from);
    let here = has_span(ast, from, span);
    let found = ast.child_nodes(from).into_iter().any(|n| node_path(ast, n, span, path));
    if !found && !here {
        path.pop();
    }
    found || here
}

/// The number of trees at or under `from` from the start to the end of `span`.
fn count_spans(ast: &Ast, from: NodeId, span: Span) -> usize {
    let below: usize = ast.child_nodes(from).into_iter().map(|n| count_spans(ast, n, span)).sum();
    below + usize::from(has_span(ast, from, span))
}

fn has_span(ast: &Ast, n: NodeId, span: Span) -> bool {
    ast.span(n).start() == span.start() && ast.span(n).end() == span.end()
}
