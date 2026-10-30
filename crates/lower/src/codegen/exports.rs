// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/GenNativeExports.scala
//! `@exported` methods of objects: extern functions calling them, arguments boxed and result
//! unboxed (nscplugin's `genTopLevelExports`).

use super::Gen;
use super::attrs::canonical;
use crate::symbols::is_static_module;
use crate::{Result, unsupported};
use nir::sig::Sig;
use nir::{Attr, Defn, DefnKind, Type, Val};
use tasty::SymbolId;
use tasty::tags::{PRIVATE, PROTECTED};
use typer::tpd::{TNodeId, Template, Tree};

const EXPORTED: &str = "scala.scalanative.unsafe.exported";

impl Gen<'_, '_> {
    /// The extern functions of the `@exported` methods of class `td`, symbol `owner`.
    pub fn gen_exports(&mut self, td: TNodeId, owner: SymbolId) -> Result<()> {
        let Some(Template { body, .. }) = self.l.tpd.class_template(td) else { return Ok(()) };
        for &stat in self.l.tpd.items(body).to_vec().iter() {
            let Tree::DefDef { .. } = self.l.tpd.tree(stat) else { continue };
            let Some(m) = self.l.tpd.sym(stat) else { continue };
            if !self.l.u.is_annotated(m, EXPORTED) {
                continue;
            }
            let span = self.l.tpd.span(stat);
            if !is_static_module(self.l.u, owner) {
                return unsupported(span, "@exported members of a class or trait");
            }
            if crate::symbols::has(self.l.u, m, PRIVATE) || crate::symbols::has(self.l.u, m, PROTECTED) {
                return unsupported(span, "@exported members that are not public");
            }
            let defn = self.gen_export(owner, m, span)?;
            self.defns.push(defn);
        }
        Ok(())
    }

    /// The extern function of `@exported` method `m` of object `owner`.
    fn gen_export(&mut self, owner: SymbolId, m: SymbolId, span: syntax::ast::Span) -> Result<Defn> {
        let class = self.l.u.annotations(m).iter().copied().find(|&c| self.l.u.full_name(c) == EXPORTED);
        let given = class.and_then(|c| self.l.u.annotation_strings(m, c).first().cloned().flatten());
        let id = match given {
            Some(id) => id,
            None => self.extern_id(m, span)?,
        };
        let ty = self.extern_call_sig(m, span)?;
        let Type::Function(params, ret) = self.pool.ty(ty).clone() else { unreachable!("a method type") };
        let orig_params = self.param_types(m);
        let saved = self.at_span(span);
        let body = self.closure_body(|g| {
            let params: Vec<_> = params.iter().map(|&t| g.param(t)).collect();
            let entry = g.fresh();
            g.label(entry, params.clone());
            let mut args = Vec::new();
            for (p, &orig) in params.iter().zip(&orig_params) {
                let v = g.val(Val::Local(p.id, p.ty));
                args.push(g.extern_result(orig, v));
            }
            let module = g.gen_module(owner);
            let result = g.call_method(m, true, module, args);
            let result = g.extern_arg(ret, result);
            g.ret(result);
        });
        let pos = self.m.pos;
        self.m.pos = saved;
        let class = self.type_name(owner);
        let name = self.pool.member(class, &Sig::Extern(id));
        let attrs = canonical(vec![Attr::NoInline, Attr::Extern { blocking: false }]);
        Ok(Defn { name, attrs, pos, kind: DefnKind::Define { ty, body } })
    }
}
