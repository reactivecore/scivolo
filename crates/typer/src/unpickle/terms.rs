//! Terms (dotty's `readTree`): the tags of the subset read, each typed as `TypeAssigner` types
//! it; the others refused.

use super::{Refusal, Unpickler};
use crate::tpd::{TNodeId, Tree as T};
use tasty::NodeId;
use tasty::tags::*;

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
            LAMBDA => self.closure(id)?,
            INLINED => self.inlined(id)?,
            NEW => {
                let tpt = self.tpt(k[0])?;
                let tpe = self.tpd.tpe(tpt);
                self.bare(T::New { tpt }, Some(tpe), None)
            }
            MATCH => self.match_(id)?,
            TRY => self.try_(id)?,
            THROW => self.throw(id)?,
            BIND => self.bind(id)?,
            ALTERNATIVE => self.alternative(id)?,
            UNAPPLY => self.unapply(id)?,
            RETURN => return Err("return in a library inline body"),
            REPEATED => {
                let elemtpt = self.tpt(k[0])?;
                let elems = k[1..].iter().map(|&e| self.term(e)).collect::<Result<Vec<_>, _>>()?;
                let seq = self.class_symbol("scala.collection.immutable.Seq")?;
                let elem = self.tpd.tpe(elemtpt);
                let tpe = crate::defn::applied(self.u, seq, vec![elem]);
                let elems = self.tpd.list(&elems);
                self.bare(T::SeqLiteral { elems, elemtpt }, Some(tpe), None)
            }
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
}
