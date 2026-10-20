// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/AdaptLazyVals.scala
//! The arguments of calls.

use super::Gen;
use crate::{Result, unsupported};
use nir::ValId;
use syntax::ast::Span;
use tasty::SymbolId;
use typer::tpd::TNodeId;

impl Gen<'_, '_> {
    /// The arguments of a call of `sym`, each of its parameter's type; a field pointer for the
    /// field `objCAS` sets.
    pub fn gen_args(&mut self, sym: SymbolId, args: &[TNodeId], span: Span) -> Result<Vec<ValId>> {
        let params = self.param_types(sym);
        if params.len() != args.len() {
            return unsupported(span, "a call with several argument lists");
        }
        let mut out = Vec::new();
        for (i, (&a, param)) in args.iter().zip(params).enumerate() {
            if i == 0 && self.l.u.symbol(sym).name == "objCAS" && sym == self.l.lazy_vals()?.obj_cas {
                out.push(self.gen_field_ptr(a)?);
                continue;
            }
            let v = self.gen_expr(a)?;
            self.check_conforms(v, param, self.l.tpd.span(a))?;
            out.push(v);
        }
        Ok(out)
    }
}
