//! Guarded inlining of virtual calls. After `TIER_UP` calls a method is lowered again, and each
//! virtual call whose site has resolved to a method is split: a direct call of that method,
//! which the inliner may then inline, taken when the receiver's method is that one, and the
//! virtual call otherwise.

use super::code::{Code, Instr};
use super::inline::max_local;
use super::{Func, FuncId};
use crate::Vm;
use nir::{Body, Comp, Defn, DefnKind, GlobalId, Inst, Local, Next, Op, Param, Type, Val, ValId};
use rustc_hash::FxHashMap;
use std::rc::Rc;

/// Calls of a method before it is lowered again with its call sites' targets.
pub const TIER_UP: u32 = 1000;

impl Vm {
    /// Lowers `f` again, with inlining and its virtual call sites' targets.
    pub fn tier_up(&mut self, f: FuncId) {
        let Func::Nir { code: Some(code), .. } = &self.funcs.all[f as usize] else { return };
        let code = code.clone();
        let defn = self.speculated(&code);
        let (defn, inlined) = self.inline(defn);
        let mut new = self.lower_code(defn);
        new.calls.set(TIER_UP + 1);
        new.tier = 1;
        new.inlined = inlined;
        if let Func::Nir { code: slot, retired, .. } = &mut self.funcs.all[f as usize] {
            *retired = slot.replace(Rc::new(new));
        }
    }

    /// The definition of `code` with its virtual calls split by the targets they resolved to.
    fn speculated(&mut self, code: &Code) -> Rc<Defn> {
        let targets = self.site_targets(code);
        let DefnKind::Define { ty, body } = &code.defn.kind else { return code.defn.clone() };
        if targets.is_empty() {
            return code.defn.clone();
        }
        let mut body = body.clone();
        self.devirtualize(&mut body);
        let body = self.speculate(&body, &targets);
        Rc::new(Defn { kind: DefnKind::Define { ty: *ty, body }, ..(*code.defn).clone() })
    }

    /// Method `g`'s definition `defn`, speculated like its first code if that ran.
    pub fn speculated_callee(&mut self, g: GlobalId, defn: Rc<Defn>) -> Rc<Defn> {
        let Some(&f) = self.funcs.by_name.get(&g) else { return defn };
        let (Func::Nir { retired: Some(code), .. } | Func::Nir { code: Some(code), retired: None, .. }) =
            &self.funcs.all[f as usize]
        else {
            return defn;
        };
        match Rc::ptr_eq(&code.defn, &defn) {
            true => self.speculated(&code.clone()),
            false => defn,
        }
    }

    /// The method each virtual call of `code` resolved to last, by NIR instruction index.
    fn site_targets(&self, code: &Code) -> FxHashMap<usize, GlobalId> {
        let mut targets = FxHashMap::default();
        for (instr, &origin) in code.instrs.iter().zip(code.origin.iter()) {
            let Instr::Invoke { site, .. } = instr else { continue };
            if code.sites[*site as usize].get() == 0 {
                continue;
            }
            let target = code.sites[*site as usize + 1].get() as FuncId;
            if let Some(Func::Nir { name, .. }) = self.funcs.all.get(target as usize) {
                targets.insert(origin as usize, *name);
            }
        }
        targets
    }

    /// `body` with each call at an index of `targets` split on whether its receiver's method
    /// is the target.
    fn speculate(&mut self, body: &Body, targets: &FxHashMap<usize, GlobalId>) -> Body {
        let mut next = max_local(body);
        let mut fresh = || {
            next += 1;
            Local(next)
        };
        let ptr_ty = nir::TypeId(self.world.pool.types.intern(Type::Ptr));
        let (mut insts, mut positions) = (Vec::new(), Vec::new());
        for (at, (inst, &pos)) in body.insts.iter().zip(&body.positions).enumerate() {
            let (Some(&target), Inst::Let { id, op: Op::Call { ty, ptr, args }, unwind, scope }) =
                (targets.get(&at), inst)
            else {
                insts.push(inst.clone());
                positions.push(pos);
                continue;
            };
            if !matches!(self.world.pool.val(*ptr), Val::Local(..)) {
                insts.push(inst.clone());
                positions.push(pos);
                continue;
            }
            let Type::Function(_, ret) = self.world.pool.ty(*ty).clone() else { unreachable!("a call") };
            let pool = &mut self.world.pool;
            let direct = ValId(pool.vals.intern(Val::Global(target, *ty)));
            let (same, fast, slow, join) = (fresh(), fresh(), fresh(), fresh());
            let (r_fast, r_slow) = (fresh(), fresh());
            let local = |l: Local, pool: &mut nir::Pool, ty| ValId(pool.vals.intern(Val::Local(l, ty)));
            let compare = Op::Comp { comp: Comp::Ieq, ty: ptr_ty, l: *ptr, r: direct };
            let bool_ty = nir::TypeId(pool.types.intern(Type::Bool));
            let cond = local(same, pool, bool_ty);
            let (vf, vs) = (local(r_fast, pool, ret), local(r_slow, pool, ret));
            let call = |ptr| Op::Call { ty: *ty, ptr, args: args.clone() };
            let split = [
                Inst::Let { id: same, op: compare, unwind: Next::None, scope: *scope },
                Inst::If(cond, Next::Label(fast, Vec::new()), Next::Label(slow, Vec::new())),
                Inst::Label(fast, Vec::new()),
                Inst::Let { id: r_fast, op: call(direct), unwind: unwind.clone(), scope: *scope },
                Inst::Jump(Next::Label(join, vec![vf])),
                Inst::Label(slow, Vec::new()),
                Inst::Let { id: r_slow, op: call(*ptr), unwind: unwind.clone(), scope: *scope },
                Inst::Jump(Next::Label(join, vec![vs])),
                Inst::Label(join, vec![Param { id: *id, ty: ret }]),
            ];
            positions.extend(std::iter::repeat_n(pos, split.len()));
            insts.extend(split);
        }
        Body { insts, positions, ..body.clone() }
    }
}
