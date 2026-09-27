//! Turns virtual calls into static ones where the receiver's class is known: a module, a final
//! class or an exact type. Resolution reads definitions only; nothing is linked.

use crate::Vm;
use nir::{Attr, Body, DefnKind, GlobalId, Inst, Next, Op, StrId, Type, Val, ValId};
use rustc_hash::FxHashMap;

impl Vm {
    /// Rewrites calls through `method` on receivers of a known class to calls of the method
    /// itself; returns whether any changed.
    pub fn devirtualize(&mut self, body: &mut Body) -> bool {
        let mut methods: FxHashMap<u64, (ValId, StrId)> = FxHashMap::default();
        for inst in &body.insts {
            if let Inst::Let { id, op: Op::Method { obj, sig }, .. } = inst {
                methods.insert(id.0, (*obj, *sig));
            }
        }
        let mut changed = false;
        let mut modules = Vec::new();
        for inst in &mut body.insts {
            let Inst::Let { op: Op::Call { ty, ptr, .. }, .. } = inst else { continue };
            let Val::Local(l, _) = *self.world.pool.val(*ptr) else { continue };
            let Some(&(obj, sig)) = methods.get(&l.0) else { continue };
            let Some((target, module)) = self.static_target(obj, sig) else { continue };
            *ptr = ValId(self.world.pool.vals.intern(Val::Global(target, *ty)));
            if module {
                modules.push((l.0, obj));
            }
            changed = true;
        }
        let pool = &self.world.pool;
        let uses = |l: u64| -> usize {
            let is = |v: &ValId| matches!(pool.val(*v), Val::Local(x, _) if x.0 == l);
            body.insts.iter().map(|i| i.operands().iter().filter(|v| is(v)).count()).sum()
        };
        let unused: Vec<(u64, ValId)> = modules.iter().filter(|(l, _)| uses(*l) == 0).copied().collect();
        for inst in &mut body.insts {
            if let Inst::Let { id, op, unwind, .. } = inst
                && let Some(&(_, obj)) = unused.iter().find(|(l, _)| *l == id.0)
            {
                *op = Op::Copy(obj);
                *unwind = Next::None;
            }
        }
        changed
    }

    /// The method `sig` resolves to for receiver `obj` if its class is known, and whether that
    /// class is a module, whose instances are never null.
    fn static_target(&mut self, obj: ValId, sig: StrId) -> Option<(GlobalId, bool)> {
        let Val::Local(_, ty) = *self.world.pool.val(obj) else { return None };
        let Type::Ref { name, exact, .. } = *self.world.pool.ty(ty) else { return None };
        let defn = self.world.defn(name).ok()?;
        let module = matches!(defn.kind, DefnKind::Module { .. });
        let fixed = exact || module || defn.attrs.iter().any(|a| matches!(a, Attr::Final));
        if !fixed {
            return None;
        }
        let sig = self.world.pool.str(sig).to_owned();
        let mut class = Some(name);
        while let Some(c) = class {
            let member = self.world.member(c, &sig);
            if let Ok(d) = self.world.defn(member) {
                return matches!(d.kind, DefnKind::Define { .. }).then_some((member, module));
            }
            class = match self.world.defn(c).ok()?.kind {
                DefnKind::Class { parent, .. } | DefnKind::Module { parent, .. } => parent,
                _ => None,
            };
        }
        None
    }
}
