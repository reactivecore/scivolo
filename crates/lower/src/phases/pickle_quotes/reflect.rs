// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ReifiedReflect.scala
//! Calls of `Quotes.reflect` on a quote's `Quotes` (dotty's `ReifiedReflect`).

use crate::phases::tree_copy::{Copier, copy};
use crate::{Lower, Result};
use syntax::ast::Span;
use tasty::{SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// A `Quotes` tree, copied at each use, in its owner.
#[derive(Clone, Copy)]
pub struct Reflect {
    pub quotes: TNodeId,
    pub owner: SymbolId,
}

impl Reflect {
    /// A copy of `quotes`.
    pub fn copy(self, l: &mut Lower) -> Result<TNodeId> {
        copy(l, &mut Copier { syms: Default::default(), from: self.owner, to: self.owner }, self.quotes)
    }

    /// `quotes.reflect`.
    pub fn this(self, l: &mut Lower) -> Result<TNodeId> {
        let quotes = self.copy(l)?;
        Ok(member(l, quotes, "reflect"))
    }

    /// `quotes.reflect.module.method`.
    pub fn method(self, l: &mut Lower, module: &str, method: &str) -> Result<TNodeId> {
        let reflect = self.this(l)?;
        let module = member(l, reflect, module);
        Ok(member(l, module, method))
    }

    /// `quotes.reflect.TreeMethods.asExpr(term).asInstanceOf[Expr[tpe]]`.
    pub fn as_expr(self, l: &mut Lower, tpe: TypeId, term: TNodeId) -> Result<TNodeId> {
        let as_expr = self.method(l, "TreeMethods", "asExpr")?;
        let expr = call(l, as_expr, &[term]);
        let class = l.u.class_ref("scala.quoted.Expr");
        let quoted = l.u.intern_type(Type::Applied(class, vec![tpe]));
        Ok(cast(l, expr, quoted))
    }

    /// `quotes.reflect.asTerm(expr)`.
    pub fn as_term(self, l: &mut Lower, expr: TNodeId) -> Result<TNodeId> {
        let reflect = self.this(l)?;
        let as_term = member(l, reflect, "asTerm");
        Ok(call(l, as_term, &[expr]))
    }
}

/// `qual.name`, of the member `name` of the class of `qual`'s type.
pub fn member(l: &mut Lower, qual: TNodeId, name: &str) -> TNodeId {
    let tpe = l.u.widen(l.tpd.tpe(qual));
    let class = l.u.class_symbol(tpe).unwrap_or_else(|| panic!("a class with {name}"));
    let members = l.u.member(class, name);
    let Some(&m) = members.iter().find(|&&m| l.u.symbol(m).kind.is_term()) else {
        panic!("{name} in {}", l.u.full_name(class))
    };
    let select = l.select(qual, m, Span::NONE);
    l.positioned(select)
}

/// `fun(args)`, of the result type of `fun`.
pub fn call(l: &mut Lower, fun: TNodeId, args: &[TNodeId]) -> TNodeId {
    let method = l.u.widen(l.tpd.tpe(fun));
    let Type::Method { result, .. } = *l.u.types().get(method) else { panic!("a method") };
    let call = l.apply(fun, args, result, Span::NONE);
    l.positioned(call)
}

/// `t.asInstanceOf[tpe]`.
pub fn cast(l: &mut Lower, t: TNodeId, tpe: TypeId) -> TNodeId {
    let fun = l.select(t, l.defs.as_instance_of, Span::NONE);
    let fun = l.positioned(fun);
    let tpt = l.type_tree(tpe, Span::NONE);
    let args = l.tpd.list(&[tpt]);
    let cast = Tree::TypeApply { fun, args };
    let cast = l.tpd.node(cast, Span::NONE, Some(tpe), Some(l.defs.as_instance_of));
    l.positioned(cast)
}
