//! Type trees (dotty's `readTpt`): type identifiers, selections and applications, singleton and
//! by-name types, and types read as `TypeTree`s.

use super::{Refusal, Unpickler};
use crate::tpd::{TNodeId, Tree as T};
use crate::typer::seen_from::normalized;
use tasty::tags::*;
use tasty::{NodeId, Type};

impl Unpickler<'_> {
    /// The type tree at `id`.
    pub fn tpt(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let k = self.kids(id);
        let t = match self.tag(id) {
            SHAREDterm => {
                let target = self.ast().deref(id);
                self.tpt(target)?
            }
            IDENTtpt => {
                let tpe = self.ty(id);
                let (tpe, sym) = match self.u.types().get(tpe).clone() {
                    Type::TypeRef(p, n) => match self.u.resolve(p, &n, true) {
                        Some(s) => (self.u.intern_type(Type::Sym(Some(p), s)), Some(s)),
                        None => (tpe, None),
                    },
                    Type::Sym(_, s) => (tpe, Some(s)),
                    _ => (tpe, None),
                };
                let name = self.tname(id, true);
                self.bare(T::Ident { name }, Some(tpe), sym)
            }
            SELECTtpt => self.select(id)?,
            SINGLETONtpt => {
                let reference = self.term(k[0])?;
                let tpe = self.tpd.get(reference).tpe;
                self.bare(T::SingletonTypeTree { reference }, tpe, None)
            }
            BYNAMEtpt => {
                let result = self.tpt(k[0])?;
                let tpe = self.tpd.tpe(result);
                let tpe = self.u.intern_type(Type::ByName(tpe));
                self.bare(T::ByNameTypeTree { result }, Some(tpe), None)
            }
            APPLIEDtpt => {
                let tpt = self.tpt(k[0])?;
                let args = k[1..].iter().map(|&a| self.tpt(a)).collect::<Result<Vec<_>, _>>()?;
                let tpe = self.ty(id);
                let tpe = normalized(self.u, tpe);
                let args = self.tpd.list(&args);
                self.bare(T::AppliedTypeTree { tpt, args }, Some(tpe), None)
            }
            EXPLICITtpt => self.tpt(k[0])?,
            ANNOTATEDtpt | LAMBDAtpt | REFINEDtpt | MATCHtpt | TYPEBOUNDStpt | BLOCK | HOLE => {
                return Err("type tree of another kind in a library inline body");
            }
            _ => {
                let tpe = self.ty(id);
                self.bare(T::TypeTree { inferred: false }, Some(tpe), None)
            }
        };
        self.check_type(t)?;
        Ok(self.positioned(id, t))
    }
}
