// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenStat.scala
//! Variables of extern objects (nscplugin's `genClassFields` of an extern module).

use super::Gen;
use crate::symbols::has;
use crate::{Result, unsupported};
use nir::{Attr, Defn, DefnKind, PosId, Val};
use tasty::SymbolId;
use tasty::tags::MUTABLE;
use typer::tpd::TNodeId;

impl Gen<'_, '_> {
    /// `var v: T = extern` of extern object `c`, an extern `var` named by `v`'s `@name` or name.
    pub fn gen_extern_field(&mut self, vd: TNodeId, c: SymbolId, class_pos: PosId) -> Result<()> {
        let span = self.l.tpd.span(vd);
        let f = self.l.tpd.sym(vd).expect("a field's symbol");
        let value = self.l.fields.get(&f).copied().unwrap_or(f);
        if !has(self.l.u, f, MUTABLE) && !has(self.l.u, value, MUTABLE) {
            return unsupported(span, "extern values");
        }
        let pos = self.sym_pos(f, c);
        let pos = if pos == self.no_pos { class_pos } else { pos };
        let attrs = self.field_attrs(f, c, vec![Attr::Extern { blocking: false }]);
        let info = self.l.u.info(f);
        let ty = self.gen_type(info);
        let rhs = self.val(Val::Zero(ty));
        let name = self.extern_name(c, value, span)?;
        self.defns.push(Defn { name, attrs, pos, kind: DefnKind::Var { ty, rhs } });
        Ok(())
    }
}
