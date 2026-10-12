//! Terms (dotty's `readTree`): the tags of the subset read, each typed as `TypeAssigner` types
//! it; the others refused.

use super::{Refusal, Unpickler};
use crate::tpd::{TNodeId, Tree as T};
use tasty::tags::*;
use tasty::{NodeId, View, view};

impl Unpickler<'_> {
    /// The term at `id`.
    pub fn term(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let k = self.kids(id);
        let t = match self.tag(id) {
            SHAREDterm => {
                let target = self.ast().deref(id);
                self.term(target)?
            }
            IDENT => self.ident(id)?,
            SELECT => self.select(id)?,
            SELECTin => self.select_in(id)?,
            QUALTHIS => self.qual_this(id)?,
            NAMEDARG => {
                let (name, arg) = (self.tname(id, false), self.term(k[0])?);
                let tpe = self.tpd.get(arg).tpe;
                self.bare(T::NamedArg { name, arg }, tpe, None)
            }
            APPLY => {
                let fun = self.term(k[0])?;
                let args = k[1..].iter().map(|&a| self.term(a)).collect::<Result<Vec<_>, _>>()?;
                let tpe = self.apply_type(fun, &args)?;
                let args = self.tpd.list(&args);
                self.bare(T::Apply { fun, args }, Some(tpe), None)
            }
            TYPEAPPLY => {
                let fun = self.term(k[0])?;
                let args = k[1..].iter().map(|&a| self.tpt(a)).collect::<Result<Vec<_>, _>>()?;
                let tpe = self.type_apply_type(fun, &args)?;
                let args = self.tpd.list(&args);
                self.bare(T::TypeApply { fun, args }, Some(tpe), None)
            }
            TYPED => {
                let (expr, tpt) = (self.term(k[0])?, self.tpt(k[1])?);
                let tpe = self.tpd.tpe(tpt);
                self.bare(T::Typed { expr, tpt }, Some(tpe), None)
            }
            ASSIGN => {
                let (lhs, rhs) = (self.term(k[0])?, self.term(k[1])?);
                let unit = self.class_type("scala.Unit")?;
                self.bare(T::Assign { lhs, rhs }, Some(unit), None)
            }
            WHILE => {
                let (cond, body) = (self.term(k[0])?, self.term(k[1])?);
                let unit = self.class_type("scala.Unit")?;
                self.bare(T::WhileDo { cond, body }, Some(unit), None)
            }
            IF if k.len() == 4 => return Err("inline if in a library inline body"),
            IF => {
                let (cond, thenp, elsep) = (self.term(k[0])?, self.term(k[1])?, self.term(k[2])?);
                let tpe = self.u.lub(self.tpd.tpe(thenp), self.tpd.tpe(elsep));
                self.bare(T::If { cond, thenp, elsep }, Some(tpe), None)
            }
            BLOCK => self.block(id)?,
            LAMBDA => self.lambda(id)?,
            INLINED => self.inlined(id)?,
            NEW => {
                let tpt = self.tpt(k[0])?;
                let tpe = self.tpd.tpe(tpt);
                self.bare(T::New { tpt }, Some(tpe), None)
            }
            MATCH => return Err("match in a library inline body"),
            TRY => return Err("try in a library inline body"),
            THROW => return Err("throw in a library inline body"),
            RETURN => return Err("return in a library inline body"),
            REPEATED => return Err("repeated arguments in a library inline body"),
            SUPER => return Err("super in a library inline body"),
            SELECTouter => return Err("outer selection in a library inline body"),
            QUOTE | SPLICE | HOLE | QUOTEPATTERN | SPLICEPATTERN => {
                return Err("quote in a library inline body");
            }
            APPLYsigpoly => return Err("signature polymorphic call in a library inline body"),
            _ => self.path(id)?,
        };
        self.check_type(t)?;
        Ok(self.positioned(id, t))
    }

    /// `INLINED`: an expansion inlined into the body, without bindings and a call.
    fn inlined(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let View::Inlined { expr, call, bindings } = view(self.ast(), id) else {
            return Err("inlined tree in a library inline body");
        };
        if call.is_some() || !bindings.is_empty() {
            return Err("inlined call in a library inline body");
        }
        let expansion = self.term(expr)?;
        let (tpe, bindings) = (self.tpd.get(expansion).tpe, self.tpd.list(&[]));
        Ok(self.bare(T::Inlined { call: None, bindings, expansion }, tpe, None))
    }

    /// `LAMBDA`: the closure of a local method, typed by its target or as a function.
    fn lambda(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let k = self.kids(id);
        let meth = self.term(k[0])?;
        let tpt = k.get(1).map(|&t| self.tpt(t)).transpose()?;
        let tpe = match tpt {
            Some(t) => self.tpd.tpe(t),
            None => self.function_type(meth)?,
        };
        let env = self.tpd.list(&[]);
        Ok(self.bare(T::Closure { env, meth, tpt }, Some(tpe), None))
    }
}
