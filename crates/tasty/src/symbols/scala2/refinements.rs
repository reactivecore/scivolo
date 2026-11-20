// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/unpickleScala2/Scala2Unpickler.scala
//! Refinement types of a pickle (dotty's `REFINEDtpe` case of `readType`): the parents
//! intersected, refined by the type members of the refinement class, and type members selected
//! from one (`({ type L[A] = F[A] })#L`).

use super::super::Universe;
use super::super::encoded_names::decode;
use super::Unpickler;
use super::format::{ALIASsym, NONEsym, TYPEsym, VALsym};
use crate::types::{Type, TypeId};
use rustc_hash::FxHashMap;

impl Unpickler {
    /// Reads a `REFINEDtpe` body ending at `end`: its parents intersected, refined by each
    /// declaration of its refinement class. Refuses a term member (a structural type) and a
    /// reference to the refinement's `this`, which a `RecType` would bind.
    pub fn refined(&mut self, u: &mut Universe, end: usize) -> Result<TypeId, String> {
        let class = self.buf.nat()?;
        let parents = self.refs(end)?;
        let mut t: Option<TypeId> = None;
        for p in parents {
            let parent = self.ty(u, p)?;
            t = Some(t.map_or(parent, |l| u.types.intern(Type::And(l, parent))));
        }
        let mut t = t.ok_or("a refinement without parents")?;
        for decl in self.refinement_decls(class)? {
            let h = self.header(decl)?;
            let info = match h.tag {
                ALIASsym => {
                    let alias = self.ty(u, h.info)?;
                    u.types.intern(Type::Alias(alias))
                }
                TYPEsym => self.ty(u, h.info)?,
                _ => return Err(format!("a structural member `{}`", decode(&h.name))),
            };
            let name = u.types.intern_name(&decode(&h.name));
            t = u.types.intern(Type::Refined(t, name, info));
        }
        Ok(t)
    }

    /// The type member named by symbol entry `sym` selected from refinement type entry `pre`,
    /// applied to `args` and dealiased (dotty's `select` by name for members of a refinement
    /// class).
    pub fn refinement_select(
        &mut self,
        u: &mut Universe,
        pre: usize,
        sym: usize,
        args: &[TypeId],
    ) -> Result<TypeId, String> {
        let prefix = self.ty(u, pre)?;
        let name = u.types.intern_name(&decode(&self.header(sym)?.name));
        let tycon = u.types.intern(Type::TypeRef(prefix, name));
        let t = if args.is_empty() { tycon } else { u.types.intern_applied(tycon, args) };
        Ok(u.dealias(t))
    }

    /// The symbol entries the refinement class of entry `class` declares, in pickle order.
    fn refinement_decls(&mut self, class: usize) -> Result<Vec<usize>, String> {
        if self.owned.is_none() {
            let mut owned: FxHashMap<usize, Vec<usize>> = FxHashMap::default();
            for i in 0..self.index.len() {
                if (NONEsym + 1..=VALsym).contains(&self.tag(i)?) {
                    let owner = self.header(i)?.owner;
                    owned.entry(owner).or_default().push(i);
                }
            }
            self.owned = Some(owned);
        }
        let owned = self.owned.as_ref().and_then(|o| o.get(&class));
        Ok(owned.cloned().unwrap_or_default())
    }
}
