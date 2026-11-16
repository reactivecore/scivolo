// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala

use super::super::builtin::synthetic;
use super::super::{Kind, SymbolId, Universe};
use crate::tags::SYNTHETIC;
use crate::types::Type;

impl Universe {
    /// Makes new parameters of method `m` of a class file, which has none, from its type, outside
    /// its declarations: a symbol per type and value parameter, of all clauses in order, the type
    /// parameters' references in the later clauses replaced by references to their symbols
    /// (dotty's `recurWithoutParamss`, new symbols on each call).
    pub fn classfile_params(&mut self, m: SymbolId) -> Vec<SymbolId> {
        let mut all = Vec::new();
        let mut t = self.info(m);
        loop {
            let (binder, params, result, kind) = match *self.ty(t) {
                Type::Poly { binder, params, result } => (binder, params, result, Kind::TypeParam),
                Type::Method { binder, params, result, .. } => (binder, params, result, Kind::Param),
                _ => return all,
            };
            let params = self.params_of(params).to_vec();
            let names: Vec<_> = params.iter().map(|&(n, _)| n).collect();
            let syms: Vec<SymbolId> = params
                .iter()
                .map(|&(n, _)| {
                    let name = self.types.name(n).to_owned();
                    let p = synthetic(self, m, &name, kind, &[SYNTHETIC]);
                    self.unenter(p);
                    p
                })
                .collect();
            let refs: Vec<_> = syms.iter().map(|&p| self.types.intern(Type::Sym(None, p))).collect();
            for (&p, &(_, info)) in syms.iter().zip(&params) {
                let info =
                    if kind == Kind::TypeParam { self.subst(info, binder, &names, &refs) } else { info };
                self.infos.insert(p, info);
            }
            t = if kind == Kind::TypeParam { self.subst(result, binder, &names, &refs) } else { result };
            all.extend(syms);
        }
    }
}
