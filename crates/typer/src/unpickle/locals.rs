// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreeUnpickler.scala
//   compiler/src/dotty/tools/dotc/typer/TypeAssigner.scala
//! Blocks with their local definitions, closures of local methods and inlined trees with their
//! bindings, typed as `TypeAssigner` types them: a type referring to the locals avoided.

use super::{Refusal, Unpickler};
use crate::tpd::{TNodeId, Tree as T};
use tasty::tags::*;
use tasty::{NodeId, SymbolId, Type, TypeId, View, view};

impl Unpickler<'_> {
    /// `BLOCK`: its local values and methods defined first, so that they may refer to each other.
    pub fn block(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let k = self.kids(id);
        let locals = self.define_locals(&k[1..])?;
        let mut stats = Vec::new();
        for &s in &k[1..] {
            stats.push(match self.tag(s) {
                VALDEF | DEFDEF => self.local_def(s)?,
                TYPEDEF => self.type_def(s)?,
                IMPORT | EXPORT => continue,
                _ => self.term(s)?,
            });
        }
        let expr = self.term(k[0])?;
        let tpe = self.avoiding(self.tpd.tpe(expr), &locals)?;
        let stats = self.tpd.list(&stats);
        Ok(self.bare(T::Block { stats, expr }, Some(tpe), None))
    }

    /// Defines the symbols of the local values and methods among statements `stats`.
    fn define_locals(&mut self, stats: &[NodeId]) -> Result<Vec<SymbolId>, Refusal> {
        let mut locals = Vec::new();
        for &s in stats {
            match self.tag(s) {
                VALDEF | DEFDEF | TYPEDEF => locals.push(self.define_local(s)?),
                _ => {}
            }
        }
        Ok(locals)
    }

    /// `INLINED`: an expansion inlined into the body, after the definitions binding its
    /// arguments; its call a trace.
    pub fn inlined(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let View::Inlined { expr, call, bindings } = view(self.ast(), id) else {
            return Err("inlined tree in a library inline body");
        };
        let bindings = bindings.to_vec();
        let locals = self.define_locals(&bindings)?;
        let bindings = bindings.into_iter().map(|b| self.local_def(b)).collect::<Result<Vec<_>, _>>()?;
        let call = match call {
            Some(c) if matches!(self.tag(self.ast().deref(c)), TYPEREFdirect | TYPEREFsymbol | TYPEREF) => {
                Some(self.tpt(c)?)
            }
            Some(_) => return Err("inlined call in a library inline body"),
            None => None,
        };
        let expansion = self.term(expr)?;
        let tpe = self.avoiding(self.tpd.tpe(expansion), &locals)?;
        let bindings = self.tpd.list(&bindings);
        Ok(self.bare(T::Inlined { call, bindings, expansion }, Some(tpe), None))
    }

    /// `LAMBDA`: the closure of a local method, typed by its target or as a function.
    pub fn closure(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let View::Lambda { meth, target } = view(self.ast(), id) else {
            return Err("closure of another kind in a library inline body");
        };
        let meth = self.term(meth)?;
        let tpt = target.map(|t| self.tpt(t)).transpose()?;
        let tpe = match tpt {
            Some(t) => self.tpd.tpe(t),
            None => self.function_type(meth)?,
        };
        let env = self.tpd.list(&[]);
        Ok(self.bare(T::Closure { env, meth, tpt }, Some(tpe), None))
    }

    /// `tpe`, the type of an expression after definitions of `locals`, widened if it refers to
    /// them (dotty's `avoid`); refused if that does not avoid them.
    pub fn avoiding(&mut self, tpe: TypeId, locals: &[SymbolId]) -> Result<TypeId, Refusal> {
        let mentions = |u: &mut tasty::Universe, t: TypeId| {
            let mut found = false;
            u.map_type(t, &mut |u, part| {
                found |= matches!(*u.types().get(part), Type::Sym(_, x) if locals.contains(&x));
                None
            });
            found
        };
        if !mentions(self.u, tpe) {
            return Ok(tpe);
        }
        let widened = self.u.widen(tpe);
        if !mentions(self.u, widened) {
            return Ok(widened);
        }
        self.class_parents_avoiding(widened, locals)
            .ok_or("type referring to locals in a library inline body")
    }
}
