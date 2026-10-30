// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The terms of `quotes.reflect`.

use super::Fallible;
use super::decode::NO_ARGUMENTS;
use crate::host::Host;
use std::collections::HashMap;
use syntax::ast::TName;
use tasty::{SymbolId, TypeId};
use typer::tpd::{TNodeId, Tpd, Tree as T, map_children};

impl Host {
    pub fn underlying(&mut self, t: TNodeId) -> TNodeId {
        self.reflect().underlying(t)
    }

    pub fn underlying_argument(&mut self, t: TNodeId) -> TNodeId {
        self.reflect().underlying_argument(t)
    }

    /// The type of `t`, of its argument for a named argument without one.
    pub fn tpe(&mut self, mut t: TNodeId) -> Fallible<TypeId> {
        while let (None, T::NamedArg { arg, .. }) = (self.tpd.get(t).tpe, self.tpd.tree(t)) {
            t = *arg;
        }
        match (self.tpd.get(t).tpe, self.tpd.tree(t)) {
            (Some(tpe), _) => Ok(tpe),
            (None, &T::SeqLiteral { elemtpt, .. }) => {
                let seq = self.u.class_ref("scala.collection.immutable.Seq");
                let elem = self.tpd.tpe(elemtpt);
                Ok(self.u.intern_type(tasty::Type::Applied(seq, vec![elem])))
            }
            _ => Err("the type of a tree without one"),
        }
    }

    pub fn is_expr_of(&mut self, t: TNodeId, of: TypeId) -> Fallible<bool> {
        let tpe = self.tpd.get(t).tpe.ok_or("isExprOf of a tree without a type")?;
        let holds = self.u.is_subtype_in(tpe, of, &mut tasty::NoVars);
        self.decided(holds, [tpe, of])
    }

    pub fn type_tree(&mut self, t: TypeId) -> TNodeId {
        self.reflect().type_tree(t)
    }

    /// `Repeated` of all but the last of `trees`, the last the element type.
    pub fn repeated(&mut self, trees: Vec<TNodeId>) -> Fallible<TNodeId> {
        let (&elemtpt, elems) = trees.split_last().ok_or(NO_ARGUMENTS)?;
        Ok(self.reflect().repeated(elems, elemtpt))
    }

    pub fn ref_(&mut self, s: SymbolId) -> Fallible<TNodeId> {
        self.reflect().ref_(s)
    }

    /// `Closure(meth, None)` of reference `meth` to a method.
    pub fn closure_of(&mut self, meth: TNodeId) -> Fallible<TNodeId> {
        let s = self.tpd.sym(meth).ok_or("Closure.apply of a term without a symbol")?;
        self.reflect().closure_of(s)
    }

    pub fn ident_ref(&mut self, tpe: TypeId) -> Fallible<TNodeId> {
        let tasty::Type::Sym(prefix, s) = *self.u.types().get(tpe) else {
            return Err("Ident.apply of a reference the frontend did not resolve");
        };
        if let Some(p) = prefix
            && let tasty::Type::Sym(_, ps) = *self.u.types().get(p)
            && self.u.symbol(ps).kind.is_term()
            && !(self.u.is_module(ps) && self.u.is_static_symbol(ps))
            && self.u.symbol(ps).kind != tasty::Kind::Package
        {
            let qual = self.ident_ref(p)?;
            return self.reflect().select(qual, s);
        }
        let local = self.u.symbol(s).owner.is_some_and(|o| self.u.symbol(o).kind.is_term());
        match local {
            true => self.reflect().local_ident(s),
            false => self.reflect().ref_(s),
        }
    }

    pub fn select_unique(&mut self, qual: TNodeId, name: String) -> Fallible<TNodeId> {
        let tpe = self.tpd.tpe(qual);
        let found = self.u.member_of_type(tpe, &name);
        let terms: Vec<_> = found.into_iter().filter(|&s| self.u.symbol(s).kind.is_term()).collect();
        let [m] = terms[..] else { return Err("Select.unique of no or an overloaded member") };
        self.reflect().select(qual, m)
    }

    pub fn select(&mut self, qual: TNodeId, s: SymbolId) -> Fallible<TNodeId> {
        self.reflect().select(qual, s)
    }

    pub fn applied_to_types(&mut self, fun: TNodeId, targs: Vec<TypeId>) -> Fallible<TNodeId> {
        self.reflect().applied_to_types(fun, &targs)
    }

    pub fn match_(&mut self, selector: TNodeId, cases: Vec<TNodeId>) -> TNodeId {
        self.reflect().match_(selector, &cases)
    }

    pub fn apply(&mut self, fun: TNodeId, args: Vec<TNodeId>) -> Fallible<TNodeId> {
        self.reflect().apply(fun, &args)
    }

    pub fn typed(&mut self, expr: TNodeId, tpts: Vec<TNodeId>) -> Fallible<TNodeId> {
        let &[tpt] = &tpts[..] else { return Err("Typed.apply without its type tree") };
        Ok(self.reflect().typed(expr, tpt))
    }

    pub fn case_def(&mut self, pat: TNodeId, guard: Option<TNodeId>, body: TNodeId) -> TNodeId {
        self.reflect().case_def(pat, guard, body)
    }

    pub fn wildcard(&mut self) -> TNodeId {
        self.reflect().wildcard()
    }

    pub fn assign(&mut self, lhs: TNodeId, rhs: TNodeId) -> TNodeId {
        let unit = self.u.class_ref("scala.Unit");
        self.tpd.node(T::Assign { lhs, rhs }, self.expansion, Some(unit), None)
    }

    pub fn named_arg(&mut self, arg: TNodeId, name: String) -> TNodeId {
        let name = TName { name: self.names.intern(&name), is_type: false };
        let tpe = self.tpd.get(arg).tpe;
        self.tpd.node(T::NamedArg { name, arg }, self.expansion, tpe, None)
    }

    /// The definition of a value named `name` of `rhs` with `flags` and a reference to it.
    pub fn let_(
        &mut self,
        owner: SymbolId,
        rhs: TNodeId,
        flags: u64,
        name: String,
    ) -> Fallible<[TNodeId; 2]> {
        if owner != self.owner && owner != self.macro_owner() {
            return Err("ValDef.let of an owner other than the splice's");
        }
        let owner = self.owner;
        let (vdef, r) = self.reflect().let_(owner, &name, rhs, flags)?;
        Ok([vdef, r])
    }

    /// `t` with its holes filled, beta-reduced; `None` if it does not reduce.
    pub fn beta_reduce(&mut self, t: TNodeId) -> Fallible<Option<TNodeId>> {
        let fills: HashMap<_, _> = self.hole_fills().into_iter().filter_map(|(h, f)| Some((h, f?))).collect();
        let filled = filled(&mut self.tpd, &fills, t);
        let owner = self.owner;
        self.reflect().beta_reduce(filled, owner)
    }

    /// `Block` of the first of `trees` as its expression and the others as its statements.
    pub fn block(&mut self, trees: Vec<TNodeId>) -> Fallible<TNodeId> {
        let (&expr, stats) = trees.split_first().ok_or("Block.apply without its expression")?;
        self.reflect().block(stats, expr)
    }
}

/// `t` with the hole placeholders in it replaced by the trees in `fills` filling them, the trees
/// on their paths copied.
fn filled(tpd: &mut Tpd, fills: &HashMap<TNodeId, TNodeId>, t: TNodeId) -> TNodeId {
    if let Some(&f) = fills.get(&t) {
        return filled(tpd, fills, f);
    }
    let kids = tpd.children(t).to_vec();
    let new: Vec<_> = kids.iter().map(|&c| filled(tpd, fills, c)).collect();
    if new == kids {
        return t;
    }
    let n = tpd.node(tpd.tree(t).clone(), tpd.span(t), tpd.get(t).tpe, tpd.sym(t));
    let mut new = new.into_iter();
    map_children(tpd, n, &mut |_, c| new.next().unwrap_or(c));
    n
}
