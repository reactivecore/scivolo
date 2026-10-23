// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreeUnpickler.scala
//! Type trees (dotty's `readTpt`): type identifiers, selections and applications, singleton and
//! by-name and annotated types, and types read as `TypeTree`s.

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
                let tpe = match k[1..].iter().any(|&a| self.tag(a) == BIND) {
                    true => {
                        let arg_types = args.iter().map(|&a| self.tpd.tpe(a)).collect();
                        self.u.intern_type(Type::Applied(self.tpd.tpe(tpt), arg_types))
                    }
                    false => self.ty(id),
                };
                let tpe = normalized(self.u, tpe);
                let args = self.tpd.list(&args);
                self.bare(T::AppliedTypeTree { tpt, args }, Some(tpe), None)
            }
            EXPLICITtpt => self.tpt(k[0])?,
            BIND => self.type_bind(id)?,
            TYPEBOUNDStpt => {
                let lo = self.tpt(k[0])?;
                let hi = k.get(1).map(|&h| self.tpt(h)).transpose()?.unwrap_or(lo);
                let alias = k.get(2).map(|&a| self.tpt(a)).transpose()?;
                let tpe = self.u.intern_type(Type::Bounds(self.tpd.tpe(lo), self.tpd.tpe(hi)));
                self.bare(T::TypeBoundsTree { lo, hi, alias }, Some(tpe), None)
            }
            ANNOTATEDtpt => {
                let arg = self.tpt(k[0])?;
                let annot = self.term(k[1])?;
                let annot_tpe = self.tpd.tpe(annot);
                let tpe = self.u.intern_type(Type::Annotated(self.tpd.tpe(arg), annot_tpe));
                self.bare(T::Annotated { arg, annot }, Some(tpe), None)
            }
            LAMBDAtpt | REFINEDtpt | MATCHtpt | BLOCK | HOLE => {
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
