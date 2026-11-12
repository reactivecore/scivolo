// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! Trees (dotty's `pickleTree`): the terms and type trees of quotes without definitions.

use super::{Refusal, TreePickler};
use crate::special::throw_method;
use crate::tpd::{List, TNodeId, Tree as T, constant_type_of};
use tasty::Type;
use tasty::tags::*;

impl TreePickler<'_> {
    /// The key tree `id` is registered under: the tree it is shared with or a copy of, else itself.
    pub fn tree_key(&self, id: TNodeId) -> u32 {
        let first = self.shared.get(&id).copied().or_else(|| self.tpd.copy_of(id)).unwrap_or(id);
        first.index() as u32
    }

    /// Tree `id`, a `SHAREDterm` if pickled before and not to be pickled anew.
    pub fn tree(&mut self, id: TNodeId) -> Result<(), Refusal> {
        let at = match self.anew.remove(&id) {
            true => self.buf.buf.current_addr(),
            false => self.buf.register_tree(self.tree_key(id)),
        };
        if at != self.buf.buf.current_addr() {
            self.byte(SHAREDterm);
            self.buf.write_ref(at);
            return Ok(());
        }
        if let Some((index, args)) = self.holes.get(&id).cloned() {
            return self.hole(id, index, &args);
        }
        let node = self.tpd.get(id);
        let (tpe, sym) = (node.tpe, node.sym);
        let typed = || tpe.ok_or("a tree without a type");
        match node.tree.clone() {
            T::Ident { name } => self.ident(id, name, typed()?, sym),
            T::Select { qualifier, name } => self.select(id, qualifier, name, sym),
            T::Apply { fun, args } if self.tpd.sym(fun).is_some_and(|f| f == throw_method(self.u)) => {
                self.byte(THROW);
                self.tree(self.tpd.items(args)[0])
            }
            T::Apply { fun, args } => self.apply_(fun, args),
            T::TypeApply { fun, args } => self.type_apply(fun, args),
            T::Literal { value } => {
                let t = typed()?;
                match *self.u.ty(t) {
                    Type::Constant(..) | Type::Str(_) => self.constant(t),
                    _ => {
                        let t = constant_type_of(self.u, &value);
                        self.constant(t)
                    }
                }
            }
            T::ClassLiteral { tpe } => {
                self.byte(CLASSconst);
                self.ty(tpe, false)
            }
            T::New { tpt } => {
                self.byte(NEW);
                self.tree(tpt)
            }
            T::Typed { expr, tpt } => {
                self.share_placeholder_tpt(expr, tpt);
                self.node(TYPED, &[expr, tpt], &[])
            }
            T::NamedArg { name, arg } => {
                self.byte(NAMEDARG);
                self.name(&self.tree_name(name, None));
                self.tree(arg)
            }
            T::Assign { lhs, rhs } => self.node(ASSIGN, &[lhs, rhs], &[]),
            T::Block { stats, expr } => {
                self.pre_register(self.tpd.items(stats));
                self.node(BLOCK, &[expr], &[stats])
            }
            T::Closure { env, meth, tpt } if env.is_empty() => {
                self.node(LAMBDA, &[&[meth][..], &tpt.into_iter().collect::<Vec<_>>()].concat(), &[])
            }
            T::Match { selector, cases, kind } => self.match_(selector, cases, kind),
            T::CaseDef { pat, guard, body } => self.case_def(pat, guard, body),
            T::Try { expr, cases, finalizer } => self.try_(expr, cases, finalizer),
            T::Bind { body, .. } => self.bind(id, body),
            T::Alternative { trees } => self.node(ALTERNATIVE, &[], &[trees]),
            T::UnApply { fun, implicits, patterns } => self.unapply(id, fun, implicits, patterns),
            T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. } => self.definition(id),
            T::Template { .. } => self.template(id),
            T::PackageDef { pid, stats } => self.package(pid, stats),
            T::Import { expr, selectors, export } => self.import(expr, selectors, export),
            T::This { qual } => self.this(qual, typed()?),
            T::LambdaTypeTree { tparams, body } => {
                let tparams = self.tpd.items(tparams).to_vec();
                self.with_length(LAMBDAtpt, &mut |p| {
                    p.params(&tparams)?;
                    p.tree(body)
                })
            }
            T::TypeBoundsTree { lo, hi, alias } => self.with_length(TYPEBOUNDStpt, &mut |p| {
                p.tree(lo)?;
                match alias {
                    None if hi == lo => Ok(()),
                    None => p.tree(hi),
                    Some(a) => p.tree(hi).and_then(|_| p.tree(a)),
                }
            }),
            T::If { cond, thenp, elsep, inline } => self.with_length(IF, &mut |p| {
                if inline {
                    p.byte(INLINE);
                }
                [cond, thenp, elsep].into_iter().try_for_each(|t| p.tree(t))
            }),
            T::WhileDo { cond, body } => self.node(WHILE, &[cond, body], &[]),
            T::SeqLiteral { elems, elemtpt } => self.node(REPEATED, &[elemtpt], &[elems]),
            T::Inlined { call, bindings, expansion } => self.inlined(call, bindings, expansion),
            T::TypeTree { .. } => {
                self.type_tree = Some(id);
                let pickled = self.ty(typed()?, false);
                self.type_tree = None;
                pickled
            }
            T::SingletonTypeTree { reference } => {
                self.byte(SINGLETONtpt);
                self.tree(reference)
            }
            T::AppliedTypeTree { tpt, args } => self.node(APPLIEDtpt, &[tpt], &[args]),
            T::ByNameTypeTree { result } => {
                self.byte(BYNAMEtpt);
                self.tree(result)
            }
            T::Super { qual, mix } => self.super_(qual, mix, typed()?),
            T::Return { expr, from } => self.return_(expr, from),
            T::Annotated { arg, annot } => self.annotated_tree(arg, annot),
            T::RefinedTypeTree { tpt, refinements } => self.refined_tree(tpt, refinements),
            T::QuotePattern { bindings, body, quotes } => self.quote_pattern(id, bindings, body, quotes),
            T::SplicePattern { body, typeargs, args } => self.splice_pattern(id, body, typeargs, args),
            _ => Err("a tree outside the subset pickled"),
        }
    }

    /// A tree of tag `tag` and a length, its children `trees` and the items of `lists`.
    pub fn node(&mut self, tag: u8, trees: &[TNodeId], lists: &[List]) -> Result<(), Refusal> {
        let items = lists.iter().flat_map(|&l| self.tpd.items(l));
        let all: Vec<TNodeId> = trees.iter().chain(items).copied().collect();
        self.with_length(tag, &mut |p| all.iter().try_for_each(|&t| p.tree(t)))
    }

    /// Whether `t` is a reference to a term or package (dotty's `TermRef`).
    pub fn is_term_ref(&self, t: tasty::TypeId) -> bool {
        match *self.u.ty(t) {
            Type::Sym(_, s) => self.u.symbol(s).kind.is_term(),
            Type::TermRef(..) | Type::Package(_) | Type::PackageName(_) => true,
            _ => false,
        }
    }

    /// Shares type tree `tpt` of `(_: T)` with its placeholder parameter's `expr`, one tree in
    /// dotty's parser: the parts healing did not make a `TypeTree`.
    fn share_placeholder_tpt(&mut self, expr: TNodeId, tpt: TNodeId) {
        let Some(p) = self.tpd.sym(expr).filter(|_| matches!(self.tpd.tree(expr), T::Ident { .. })) else {
            return;
        };
        if let Some(&param_tpt) = self.param_tpts.get(&p)
            && self.u.symbol(p).flags.has(SYNTHETIC)
            && self.tpd.span(param_tpt) == self.tpd.span(tpt)
        {
            self.share_unhealed(param_tpt, tpt);
        }
    }

    /// Shares `copy` with `first`, two typings of one type tree, or else their children, as far
    /// as healing did not make them `TypeTree`s.
    fn share_unhealed(&mut self, first: TNodeId, copy: TNodeId) {
        if !self.has_type_tree(first) && !self.has_type_tree(copy) {
            self.shared.insert(copy, first);
            return;
        }
        let (a, b) = (self.tpd.children(first), self.tpd.children(copy));
        if a.len() == b.len() {
            for (f, c) in a.into_iter().zip(b) {
                self.share_unhealed(f, c);
            }
        }
    }

    fn has_type_tree(&self, t: TNodeId) -> bool {
        matches!(self.tpd.tree(t), T::TypeTree { .. })
            || self.tpd.children(t).into_iter().any(|c| self.has_type_tree(c))
    }

    /// A term hole: its index, type and arguments.
    fn hole(&mut self, id: TNodeId, index: u32, args: &[TNodeId]) -> Result<(), Refusal> {
        let tpe = self.tpd.get(id).tpe.ok_or("a hole without a type")?;
        self.with_length(HOLE, &mut |p| {
            p.nat(index as u64);
            p.ty(tpe, true)?;
            for &a in args {
                if p.tpd.is_type_tree(a) && p.is_term_ref(p.tpd.tpe(a)) {
                    p.byte(EXPLICITtpt);
                }
                p.tree(a)?;
            }
            Ok(())
        })
    }
}
