//! Inlining of small, statically called methods into their callers before lowering. A call
//! becomes a jump to the callee's renamed entry block; the callee's returns jump to a new block
//! whose parameter is the call's result, and its instructions without an exception handler get
//! the call's.

use super::rename::Renamer;
use crate::Vm;
use nir::{Attr, Body, Defn, DefnKind, GlobalId, Inst, Next, Op, Param, Type, Val, ValId};
use rustc_hash::FxHashMap;
use std::rc::Rc;

/// Instructions a callee may have to be inlined.
const MAX_CALLEE: usize = 48;
/// Instructions a caller may grow to by inlining.
const MAX_BODY: usize = 2000;
/// Passes over a caller; each inlines the calls the previous one exposed.
const ROUNDS: usize = 12;

impl Vm {
    /// `defn` with small static callees inlined, or `defn` itself if none are.
    pub fn inline(&mut self, defn: Rc<Defn>) -> Rc<Defn> {
        let DefnKind::Define { ty, body } = &defn.kind else { return defn };
        let (ty, mut body) = (*ty, body.clone());
        let mut changed = false;
        for round in 0..ROUNDS {
            if body.insts.len() > MAX_BODY || !self.inline_round(defn.name, &mut body, round == 0) {
                break;
            }
            changed = true;
        }
        match changed {
            true => Rc::new(Defn { kind: DefnKind::Define { ty, body }, ..(*defn).clone() }),
            false => defn,
        }
    }

    /// Inlines the calls of `body`; `recursive` lets the caller be inlined into itself once.
    fn inline_round(&mut self, caller: GlobalId, body: &mut Body, recursive: bool) -> bool {
        let mut changed = self.devirtualize(body);
        let mut next_local = max_local(body) + 1;
        let (mut insts, mut positions) = (Vec::new(), Vec::new());
        for (inst, pos) in body.insts.iter().zip(&body.positions) {
            let Inst::Let { id, op: Op::Call { ty, ptr, args }, unwind, .. } = inst else {
                insts.push(inst.clone());
                positions.push(*pos);
                continue;
            };
            let Some(callee) = self.inline_target(caller, *ptr, recursive) else {
                insts.push(inst.clone());
                positions.push(*pos);
                continue;
            };
            let DefnKind::Define { body: cb, .. } = &callee.kind else { unreachable!("a define") };
            let Type::Function(_, ret) = self.world.pool.ty(*ty).clone() else { unreachable!("a call") };
            let mut r = Renamer { locals: FxHashMap::default(), next: &mut next_local, vm: self };
            let cont = r.fresh();
            let Some(Inst::Label(entry, _)) = cb.insts.first() else { unreachable!("an entry block") };
            let entry = r.local(*entry);
            insts.push(Inst::Jump(Next::Label(entry, args.clone())));
            positions.push(*pos);
            for (ci, cp) in cb.insts.iter().zip(&cb.positions) {
                insts.push(r.inst(ci, unwind, cont));
                positions.push(*cp);
            }
            insts.push(Inst::Label(cont, vec![Param { id: *id, ty: ret }]));
            positions.push(*pos);
            changed = true;
        }
        body.insts = insts;
        body.positions = positions;
        changed
    }

    /// The definition of the method `ptr` statically calls, speculated, if it is small enough
    /// and safe to inline into `caller`.
    fn inline_target(&mut self, caller: GlobalId, ptr: ValId, recursive: bool) -> Option<Rc<Defn>> {
        let Val::Global(g, _) = *self.world.pool.val(ptr) else { return None };
        if g == caller && !recursive {
            return None;
        }
        let target = match self.funcs.inlinable.get(&g) {
            Some(known) => known.clone(),
            None => {
                let target = self.judge_inline(g);
                self.funcs.inlinable.insert(g, target.clone());
                target
            }
        }?;
        Some(self.speculated_callee(g, target))
    }

    /// The definition of method `g` if it is small enough and safe to inline anywhere.
    fn judge_inline(&mut self, g: GlobalId) -> Option<Rc<Defn>> {
        if self.native(g).is_some() {
            return None;
        }
        let defn = self.world.defn(g).ok()?;
        if defn.attrs.iter().any(|a| matches!(a, Attr::NoInline | Attr::Extern { .. })) {
            return None;
        }
        let defn = self.fold_linktime(defn)?;
        let DefnKind::Define { body, .. } = &defn.kind else { return None };
        (body.insts.len() <= MAX_CALLEE && body.insts.iter().all(inlinable)).then_some(defn)
    }
}

/// Whether an instruction keeps its meaning inside another method's frame.
fn inlinable(inst: &Inst) -> bool {
    !matches!(inst, Inst::Let { op: Op::Stackalloc { .. } | Op::Var(_) | Op::Dynmethod { .. }, .. })
}

pub fn max_local(body: &Body) -> u64 {
    let mut max = 0;
    for inst in &body.insts {
        match inst {
            Inst::Label(l, params) => {
                max = max.max(l.0);
                params.iter().for_each(|p| max = max.max(p.id.0));
            }
            Inst::Let { id, unwind, .. } => {
                max = max.max(id.0).max(next_max(unwind));
            }
            Inst::Jump(n) | Inst::Unreachable(n) | Inst::Throw(_, n) => max = max.max(next_max(n)),
            Inst::If(_, t, e) | Inst::LinktimeIf(_, t, e) => max = max.max(next_max(t)).max(next_max(e)),
            Inst::Switch(_, d, cases) => {
                max = max.max(next_max(d));
                cases.iter().for_each(|c| max = max.max(next_max(c)));
            }
            Inst::Ret(_) => {}
        }
    }
    max
}

fn next_max(n: &Next) -> u64 {
    match n {
        Next::None => 0,
        Next::Unwind(p, to) => p.id.0.max(next_max(to)),
        Next::Case(_, to) => next_max(to),
        Next::Label(l, _) => l.0,
    }
}
