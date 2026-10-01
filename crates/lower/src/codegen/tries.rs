//! `try`/`catch`: a handler block reached by unwinding, type tests in the order of the cases
//! (nscplugin's `genTry` and `genTryCatch`).

use super::Gen;
use crate::{Result, unsupported};
use nir::{Local, Op, Param, PosId, TypeId, Val, ValId};
use tasty::SymbolId;
use typer::tpd::{List, TNodeId, Tree};

/// A catch case: the exception type it tests, its body, the symbol it binds.
struct Case {
    ty: TypeId,
    body: TNodeId,
    bind: Option<SymbolId>,
    pos: PosId,
}

impl<'l, 'a> Gen<'l, 'a> {
    pub fn gen_try(
        &mut self,
        t: TNodeId,
        expr: TNodeId,
        cases: List,
        finalizer: Option<TNodeId>,
    ) -> Result<ValId> {
        let cases = self.l.tpd.items(cases).to_vec();
        if finalizer.is_some() {
            return unsupported(self.l.tpd.span(t), "`finally`");
        }
        if cases.is_empty() {
            return self.gen_expr(expr);
        }
        let retty = self.gen_type(self.l.tpd.tpe(t));
        let pos = self.pos_or(self.l.tpd.span(expr), self.m.pos);
        let saved = self.at(pos);
        let cases = cases.into_iter().map(|c| self.catch_case(c)).collect::<Result<Vec<_>>>()?;
        let (handler, normal, merge) = (self.fresh(), self.fresh(), self.fresh());
        let exc = self.fresh();
        let merge_v = self.fresh();
        let throwable = self.throwable_type();
        let (result, insts) = self.nested(|g| -> Result<()> {
            let outer = g.m.unwind_handler.replace(handler);
            let scope = g.enter_scope(pos);
            g.label(normal, Vec::new());
            let v = g.gen_expr(expr)?;
            g.jump_exclude_unit(retty, merge, v);
            g.m.scope = scope;
            g.m.unwind_handler = outer;
            let scope = g.enter_scope(pos);
            g.label(handler, vec![Param { id: exc, ty: throwable }]);
            let exc = g.val(Val::Local(exc, throwable));
            let v = g.catches(retty, exc, merge, &cases)?;
            g.jump_exclude_unit(retty, merge, v);
            g.m.scope = scope;
            Ok(())
        });
        result?;
        self.jump(normal, Vec::new());
        self.append(insts);
        let v = self.label_exclude_unit(merge, merge_v, retty);
        self.m.pos = saved;
        Ok(v)
    }

    fn catch_case(&mut self, c: TNodeId) -> Result<Case> {
        let span = self.l.tpd.span(c);
        let Tree::CaseDef { pat, guard: None, body } = *self.l.tpd.tree(c) else {
            return unsupported(span, "a catch case with a guard");
        };
        let pos = self.pos_or(span, self.m.pos);
        let (ty, bind) = match *self.l.tpd.tree(pat) {
            Tree::Typed { expr, tpt } if self.is_wildcard(expr) => (self.gen_type(self.l.tpd.tpe(tpt)), None),
            _ if self.is_wildcard(pat) => (self.throwable_type(), None),
            Tree::Bind { body: inner, .. }
                if self.is_wildcard(inner) || matches!(self.l.tpd.tree(inner), Tree::Typed { .. }) =>
            {
                let sym = self.l.tpd.sym(pat).expect("a bound symbol");
                let info = self.l.u.info(sym);
                (self.gen_type(info), Some(sym))
            }
            _ => return unsupported(span, "a catch pattern other than a type test"),
        };
        Ok(Case { ty, body, bind, pos })
    }

    fn is_wildcard(&self, t: TNodeId) -> bool {
        matches!(*self.l.tpd.tree(t), Tree::Ident { name } if self.l.names.text(name.name) == "_")
    }

    /// The cases in turn: a type test, the body if it holds, the remaining cases if not, a
    /// rethrow after the last.
    fn catches(&mut self, retty: TypeId, exc: ValId, merge: Local, cases: &[Case]) -> Result<ValId> {
        let Some((case, rest)) = cases.split_first() else {
            self.raise(exc);
            return Ok(self.val(Val::Unit));
        };
        let saved = self.at(case.pos);
        let cond = self.emit(Op::Is { ty: case.ty, obj: exc });
        let v = self.gen_if_with(
            retty,
            Box::new(move |_: &mut Gen<'l, 'a>| Ok(cond)),
            Box::new(move |g: &mut Gen<'l, 'a>| g.catch_body(retty, exc, merge, case)),
            Box::new(move |g: &mut Gen<'l, 'a>| g.catches(retty, exc, merge, rest)),
        );
        self.m.pos = saved;
        v
    }

    fn catch_body(&mut self, retty: TypeId, exc: ValId, merge: Local, case: &Case) -> Result<ValId> {
        let pos = self.pos_or(self.l.tpd.span(case.body), case.pos);
        let scope = self.enter_scope(pos);
        if let Some(sym) = case.bind {
            let cast = self.emit(Op::As { ty: case.ty, obj: exc });
            let Val::Local(id, _) = *self.pool.val(cast) else { unreachable!("a local") };
            let name = self.local_name(sym);
            self.name_local(id, &name);
            self.m.env.insert(sym, cast);
        }
        let v = self.gen_expr(case.body)?;
        self.jump_exclude_unit(retty, merge, v);
        self.m.scope = scope;
        Ok(self.val(Val::Unit))
    }
}
