// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeSyncAndTry.scala
//! `try`/`catch`/`finally` with exception handlers, and the cleanups a `return` runs on its way
//! out (dotty's `genLoadTry`, `pendingCleanups`).

use super::body::Body;
use super::descriptors::{class_internal, descriptor};
use super::{Gen, stash, unsupported};
use classfile::{Label, value_slots};
use lower::Result;
use tasty::{SymbolId, nme};
use typer::tpd::{List, TNodeId, Tree};

const THROWABLE: &str = "java/lang/Throwable";

/// A catch case: the class it catches, the local it binds and its body.
struct Catch {
    class: String,
    bound: Option<(SymbolId, String)>,
    body: TNodeId,
}

impl Gen<'_, '_> {
    /// `try expr catch cases finally finalizer` `t`; returns the slots of its value.
    pub fn gen_try(
        &mut self,
        b: &mut Body,
        t: TNodeId,
        expr: TNodeId,
        cases: List,
        finalizer: Option<TNodeId>,
    ) -> Result<i32> {
        let desc = self.desc_of(t);
        let slots = value_slots(&desc);
        let catches = self.l.tpd.items(cases).to_vec().into_iter().map(|c| self.catch_case(c));
        let catches = catches.collect::<Result<Vec<Catch>>>()?;
        let stash = match b.code.stack() > 0 && !catches.is_empty() {
            true => Some(self.stash(b, self.l.tpd.span(t))?),
            false => None,
        };
        let depth = b.code.stack();
        let guard = slots > 0 && finalizer.is_some_and(|f| self.contains_try(f));
        let tmp = guard.then(|| b.temp(&desc));
        let cleanup = finalizer.map(|_| b.code.new_label());
        let post = b.code.new_label();
        let start = b.code.here();
        b.cleanups.extend(cleanup);
        self.expr_as(b, expr, slots)?;
        b.cleanups.truncate(b.cleanups.len() - usize::from(cleanup.is_some()));
        nop_if_needed(b, start);
        let end = b.code.here();
        b.code.jump(0xa7, post);
        for c in catches {
            let outer = std::mem::replace(&mut b.emit_cleanup, false);
            b.code.set_stack(depth + 1);
            let handler = b.code.here();
            b.cleanups.extend(cleanup);
            match &c.bound {
                Some((sym, local)) => {
                    let slot = b.local(*sym, local.clone());
                    b.code.store(local, slot);
                }
                None => b.code.pop(),
            }
            self.expr_as(b, c.body, slots)?;
            nop_if_needed(b, handler);
            b.cleanups.truncate(b.cleanups.len() - usize::from(cleanup.is_some()));
            let class = b.pool.class(&c.class);
            b.code.protect(start, end, handler, class);
            b.code.jump(0xa7, post);
            b.emit_cleanup |= outer;
        }
        let needs_cleanup = b.emit_cleanup;
        if let Some(fin) = finalizer {
            let outer = std::mem::replace(&mut b.emit_cleanup, false);
            b.code.set_stack(depth + 1);
            let handler = b.code.here();
            b.code.protect(start, handler, handler, 0);
            let exc = b.temp("L");
            b.code.store("L", exc);
            self.expr_as(b, fin, 0)?;
            b.code.load("L", exc);
            b.code.insn(0xbf);
            b.emit_cleanup |= outer;
        }
        if let (Some(fin), Some(cleanup), true) = (finalizer, cleanup, needs_cleanup) {
            b.code.place(cleanup);
            b.code.set_stack(depth);
            self.expr_as(b, fin, 0)?;
            pending_cleanups(b);
        }
        b.code.place(post);
        b.code.set_stack(depth + slots);
        if let Some(fin) = finalizer {
            if let Some(tmp) = tmp {
                b.code.store(&desc, tmp);
            }
            self.expr_as(b, fin, 0)?;
            if let Some(tmp) = tmp {
                b.code.load(&desc, tmp);
            }
        }
        if let Some(s) = stash {
            stash::unstash(b, s, &desc, tmp);
        }
        Ok(slots)
    }

    fn catch_case(&mut self, c: TNodeId) -> Result<Catch> {
        let span = self.l.tpd.span(c);
        let Tree::CaseDef { pat, guard: None, body } = *self.l.tpd.tree(c) else {
            return unsupported(span, "a catch case with a guard");
        };
        let (class, bound) = match *self.l.tpd.tree(pat) {
            Tree::Typed { expr, tpt } if self.is_wildcard(expr) => {
                let tpe = self.l.tpd.tpe(tpt);
                (class_internal(&self.l.u.erase(tpe)), None)
            }
            _ if self.is_wildcard(pat) => (THROWABLE.to_owned(), None),
            Tree::Bind { .. } => {
                let sym = self.l.tpd.sym(pat).expect("a bound symbol");
                let info = self.l.u.info(sym);
                let erased = self.l.u.erase(info);
                (class_internal(&erased), Some((sym, descriptor(&erased))))
            }
            _ => return unsupported(span, "a catch pattern other than a type test"),
        };
        Ok(Catch { class, bound, body })
    }

    fn contains_try(&self, t: TNodeId) -> bool {
        matches!(self.l.tpd.tree(t), Tree::Try { .. })
            || self.l.tpd.children(t).into_iter().any(|c| self.contains_try(c))
    }

    pub fn is_wildcard(&self, t: TNodeId) -> bool {
        matches!(*self.l.tpd.tree(t), Tree::Ident { name } if self.l.names.text(name.name) == nme::WILDCARD)
    }
}

/// A `nop` if no instruction follows `start`, so that a protected range is not empty.
pub fn nop_if_needed(b: &mut Body, start: Label) {
    if b.code.is_here(start) {
        b.code.insn(0x00);
    }
}

/// The end of a cleanup: the next enclosing one, or the method's return of the value a `return`
/// left (dotty's `pendingCleanups`).
pub fn pending_cleanups(b: &mut Body) {
    match b.cleanups.last() {
        Some(&next) => b.code.jump(0xa7, next),
        None => {
            let result = b.result.clone();
            if let Some(slot) = b.early_return.filter(|_| result != "V") {
                b.code.load(&result, slot);
            }
            b.code.ret(if b.early_return.is_some() { &result } else { "V" });
            b.emit_cleanup = false;
        }
    }
}
