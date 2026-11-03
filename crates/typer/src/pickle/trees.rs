// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! Trees (dotty's `pickleTree`): the terms and type trees of quotes without definitions.

use super::names::sym_name;
use super::{Refusal, TreePickler};
use crate::defn::type_ref;
use crate::signatures::result_sig;
use crate::special::throw_method;
use crate::tpd::{List, TNodeId, Tree as T, constant_type_of};
use syntax::ast::TName;
use tasty::tags::*;
use tasty::write::Name;
use tasty::{SymbolId, Type, TypeId};

impl TreePickler<'_> {
    /// Tree `id`, a `SHAREDterm` if pickled before.
    pub fn tree(&mut self, id: TNodeId) -> Result<(), Refusal> {
        let first = self.shared.get(&id).copied().unwrap_or(id);
        let at = self.buf.register_tree(first.index() as u32);
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
            T::Select { qualifier, name } => self.select(qualifier, name, sym),
            T::Apply { fun, args } if self.tpd.sym(fun).is_some_and(|f| f == throw_method(self.u)) => {
                self.byte(THROW);
                self.tree(self.tpd.items(args)[0])
            }
            T::Apply { fun, args } => self.node(APPLY, &[fun], &[args]),
            T::TypeApply { fun, args } => self.node(TYPEAPPLY, &[fun], &[args]),
            T::Literal { value } => {
                let t = typed()?;
                match self.u.types().get(t) {
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
            T::Typed { expr, tpt } => self.node(TYPED, &[expr, tpt], &[]),
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
            T::Inlined { call, bindings, expansion } => {
                self.pre_register(self.tpd.items(bindings));
                let call: Vec<TNodeId> = call.into_iter().collect();
                self.node(INLINED, &[&[expansion][..], &call].concat(), &[bindings])
            }
            T::TypeTree { .. } => self.ty(typed()?, false),
            T::SingletonTypeTree { reference } => {
                self.byte(SINGLETONtpt);
                self.tree(reference)
            }
            T::AppliedTypeTree { tpt, args } => self.node(APPLIEDtpt, &[tpt], &[args]),
            T::ByNameTypeTree { result } => {
                self.byte(BYNAMEtpt);
                self.tree(result)
            }
            _ => Err("a tree outside the subset pickled"),
        }
    }

    /// A tree of tag `tag` and a length, its children `trees` and the items of `lists`.
    fn node(&mut self, tag: u8, trees: &[TNodeId], lists: &[List]) -> Result<(), Refusal> {
        let items = lists.iter().flat_map(|&l| self.tpd.items(l));
        let all: Vec<TNodeId> = trees.iter().chain(items).copied().collect();
        self.with_length(tag, &mut |p| all.iter().try_for_each(|&t| p.tree(t)))
    }

    /// The name of tree name `name`, as `sym`'s name if it is the symbol's.
    fn tree_name(&self, name: TName, sym: Option<SymbolId>) -> Name {
        let text = self.names.text(name.name);
        match sym {
            Some(s) if self.u.symbol(s).name == text => sym_name(self.u, s),
            _ => Name::utf8(text),
        }
    }

    /// Identifier `id`: a reference to a self as its class's `this`, a path as its type, else its
    /// name and type.
    fn ident(&mut self, id: TNodeId, name: TName, tpe: TypeId, sym: Option<SymbolId>) -> Result<(), Refusal> {
        if let Type::This(class) = *self.u.types().get(tpe) {
            return self.class_this(id, None, class);
        }
        if self.is_term_ref(tpe) && self.names.text(name.name) != "_" {
            return self.ty(tpe, false);
        }
        self.byte(if name.is_type { IDENTtpt } else { IDENT });
        self.name(&self.tree_name(name, sym));
        self.ty(tpe, false)
    }

    /// A selection, of a method through its owner unless the qualifier is a hole.
    fn select(&mut self, qual: TNodeId, name: TName, sym: Option<SymbolId>) -> Result<(), Refusal> {
        let method = sym.filter(|&s| !name.is_type && result_sig(self.u, s).is_some());
        let Some(s) = method.filter(|_| !self.holes.contains_key(&qual)) else {
            self.byte(if name.is_type { SELECTtpt } else { SELECT });
            match method {
                Some(s) => self.signed_name(s, true)?,
                None => self.name(&self.tree_name(name, sym)),
            }
            return self.tree(qual);
        };
        let owner = self.u.symbol(s).owner.ok_or("a method without owner")?;
        let owner = type_ref(self.u, owner);
        self.with_length(SELECTin, &mut |p| {
            p.signed_name(s, true)?;
            p.tree(qual)?;
            p.ty(owner, false)
        })
    }

    /// Whether `t` is a reference to a term or package (dotty's `TermRef`).
    fn is_term_ref(&self, t: tasty::TypeId) -> bool {
        match *self.u.types().get(t) {
            Type::Sym(_, s) => self.u.symbol(s).kind.is_term(),
            Type::TermRef(..) | Type::Package(_) | Type::PackageName(_) => true,
            _ => false,
        }
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
