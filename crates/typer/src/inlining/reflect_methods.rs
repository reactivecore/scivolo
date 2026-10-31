// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The method definitions macros build with `quotes.reflect`: `DefDef(sym, rhsFn)` with
//! parameters made from the method's type.

use super::Reflect;
use crate::tpd::TNodeId;
use tasty::{Flags, Kind, Symbol, SymbolId, Type, TypeId};

impl Reflect<'_> {
    /// New parameter definitions of method `s` by clause, from its type (the parameter symbols
    /// of dotty's `polyDefDef`): type parameters as `TypeDef`s, their bounds and later clauses
    /// referring to them.
    pub fn method_params(&mut self, s: SymbolId) -> Result<Vec<Vec<TNodeId>>, &'static str> {
        let (span, mut t) = (self.span, self.u.info(s));
        let mut clauses = Vec::new();
        loop {
            match self.u.types().get(t).clone() {
                Type::Method { params, result, .. } => {
                    let mut clause = Vec::new();
                    for (name, info) in params {
                        let p = self.new_param(s, &name, Kind::Param);
                        self.u.set_info(p, info);
                        let tpt = self.type_tree(info);
                        clause.push(self.build()?.val_def(p, tpt, None, span));
                    }
                    clauses.push(clause);
                    t = result;
                }
                Type::Poly { binder, params, result } => {
                    let (names, bounds): (Vec<String>, Vec<TypeId>) = params.into_iter().unzip();
                    let syms: Vec<SymbolId> =
                        names.iter().map(|n| self.new_param(s, n, Kind::TypeParam)).collect();
                    let refs: Vec<TypeId> = syms.iter().map(|&p| self.u.intern_type(Type::Sym(None, p))).collect();
                    let mut clause = Vec::new();
                    for (&p, b) in syms.iter().zip(bounds) {
                        let info = self.u.subst(b, binder, &names, &refs);
                        self.u.set_info(p, info);
                        clause.push(self.type_param_def(p));
                    }
                    clauses.push(clause);
                    t = self.u.subst(result, binder, &names, &refs);
                }
                _ => return Ok(clauses),
            }
        }
    }

    fn new_param(&mut self, owner: SymbolId, name: &str, kind: Kind) -> SymbolId {
        let p = self.u.define(Symbol {
            name: name.to_owned(),
            kind,
            owner: None,
            flags: Flags::default(),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        });
        self.u.set_owner(p, owner);
        p
    }

    /// `DefDef(s, rhsFn)` of method `s` with parameter definitions `params` and the right-hand
    /// side `rhsFn` made.
    pub fn def_def(
        &mut self,
        s: SymbolId,
        params: &[Vec<TNodeId>],
        rhs: Option<TNodeId>,
    ) -> Result<TNodeId, &'static str> {
        if self.u.symbol(s).kind != Kind::Def {
            return Err("DefDef.apply of a symbol other than a method");
        }
        let mut result = self.u.info(s);
        for clause in params {
            match self.u.types().get(result).clone() {
                Type::Method { result: r, .. } => result = r,
                Type::Poly { binder, params: ps, result: r } => {
                    let names: Vec<String> = ps.into_iter().map(|(n, _)| n).collect();
                    let refs: Vec<TypeId> = (clause.iter())
                        .map(|&p| self.tpd.sym(p).map(|p| self.u.intern_type(Type::Sym(None, p))))
                        .collect::<Option<_>>()
                        .ok_or("a type parameter without a symbol")?;
                    result = self.u.subst(r, binder, &names, &refs);
                }
                _ => break,
            }
        }
        let (span, tpt) = (self.span, self.type_tree(result));
        let clauses: Vec<&[TNodeId]> = params.iter().map(Vec::as_slice).collect();
        Ok(self.build()?.def_def(s, &clauses, tpt, rhs, span))
    }
}
