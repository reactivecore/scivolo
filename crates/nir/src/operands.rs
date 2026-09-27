//! The values instructions read.

use crate::nir::{Inst, Next, Op, ValId};

impl Op {
    /// Every value this operation reads, in order.
    pub fn operands(&self) -> Vec<ValId> {
        match self {
            Op::Call { ptr, args, .. } => std::iter::once(*ptr).chain(args.iter().copied()).collect(),
            Op::Load { ptr, .. } | Op::Stackalloc { n: ptr, .. } => vec![*ptr],
            Op::Store { ptr, value, .. } => vec![*ptr, *value],
            Op::Elem { ptr, indexes, .. } => std::iter::once(*ptr).chain(indexes.iter().copied()).collect(),
            Op::Extract { aggr, .. } => vec![*aggr],
            Op::Insert { aggr, value, .. } => vec![*aggr, *value],
            Op::Bin { l, r, .. } | Op::Comp { l, r, .. } => vec![*l, *r],
            Op::Conv { value, .. } => vec![*value],
            Op::Classalloc { zone, .. } => zone.iter().copied().collect(),
            Op::Fieldload { obj, .. } | Op::Field { obj, .. } => vec![*obj],
            Op::Fieldstore { obj, value, .. } => vec![*obj, *value],
            Op::Method { obj, .. } | Op::Dynmethod { obj, .. } => vec![*obj],
            Op::As { obj, .. } | Op::Is { obj, .. } | Op::Box { obj, .. } | Op::Unbox { obj, .. } => {
                vec![*obj]
            }
            Op::Copy(v) | Op::Varload(v) | Op::Arraylength(v) => vec![*v],
            Op::Varstore { slot, value } => vec![*slot, *value],
            Op::Arrayalloc { init, zone, .. } => std::iter::once(*init).chain(zone.iter().copied()).collect(),
            Op::Arrayload { arr, idx, .. } => vec![*arr, *idx],
            Op::Arraystore { arr, idx, value, .. } => vec![*arr, *idx, *value],
            Op::Fence(_) | Op::Module(_) | Op::SizeOf(_) | Op::AlignmentOf(_) | Op::Var(_) => Vec::new(),
        }
    }
}

impl Op {
    /// Calls `f` with every value this operation reads, in order.
    pub fn for_each_operand(&self, f: &mut impl FnMut(ValId)) {
        match self {
            Op::Call { ptr, args, .. } => {
                f(*ptr);
                args.iter().for_each(|a| f(*a));
            }
            Op::Elem { ptr, indexes, .. } => {
                f(*ptr);
                indexes.iter().for_each(|a| f(*a));
            }
            Op::Classalloc { zone, .. } => zone.iter().for_each(|z| f(*z)),
            Op::Arrayalloc { init, zone, .. } => {
                f(*init);
                zone.iter().for_each(|z| f(*z));
            }
            Op::Load { ptr: a, .. }
            | Op::Stackalloc { n: a, .. }
            | Op::Extract { aggr: a, .. }
            | Op::Conv { value: a, .. }
            | Op::Fieldload { obj: a, .. }
            | Op::Field { obj: a, .. }
            | Op::Method { obj: a, .. }
            | Op::Dynmethod { obj: a, .. }
            | Op::As { obj: a, .. }
            | Op::Is { obj: a, .. }
            | Op::Box { obj: a, .. }
            | Op::Unbox { obj: a, .. }
            | Op::Copy(a)
            | Op::Varload(a)
            | Op::Arraylength(a) => f(*a),
            Op::Store { ptr: a, value: b, .. }
            | Op::Insert { aggr: a, value: b, .. }
            | Op::Bin { l: a, r: b, .. }
            | Op::Comp { l: a, r: b, .. }
            | Op::Fieldstore { obj: a, value: b, .. }
            | Op::Varstore { slot: a, value: b }
            | Op::Arrayload { arr: a, idx: b, .. } => {
                f(*a);
                f(*b);
            }
            Op::Arraystore { arr, idx, value, .. } => {
                f(*arr);
                f(*idx);
                f(*value);
            }
            Op::Fence(_) | Op::Module(_) | Op::SizeOf(_) | Op::AlignmentOf(_) | Op::Var(_) => {}
        }
    }

    /// This operation with every value it reads replaced by `f` of it.
    pub fn map_operands(&self, f: &mut impl FnMut(ValId) -> ValId) -> Op {
        let mut all = |vs: &[ValId]| vs.iter().map(|v| f(*v)).collect::<Vec<_>>();
        let op = self.clone();
        let vals = self.operands();
        let mut mapped = all(&vals).into_iter();
        let mut next = move || mapped.next().expect("one mapped value per operand");
        match op {
            Op::Call { ty, args, .. } => {
                let ptr = next();
                Op::Call { ty, ptr, args: args.iter().map(|_| next()).collect() }
            }
            Op::Load { ty, order, .. } => Op::Load { ty, ptr: next(), order },
            Op::Stackalloc { ty, .. } => Op::Stackalloc { ty, n: next() },
            Op::Store { ty, order, .. } => Op::Store { ty, ptr: next(), value: next(), order },
            Op::Elem { ty, indexes, .. } => {
                let ptr = next();
                Op::Elem { ty, ptr, indexes: indexes.iter().map(|_| next()).collect() }
            }
            Op::Extract { indexes, .. } => Op::Extract { aggr: next(), indexes },
            Op::Insert { indexes, .. } => Op::Insert { aggr: next(), value: next(), indexes },
            Op::Bin { bin, ty, .. } => Op::Bin { bin, ty, l: next(), r: next() },
            Op::Comp { comp, ty, .. } => Op::Comp { comp, ty, l: next(), r: next() },
            Op::Conv { conv, ty, .. } => Op::Conv { conv, ty, value: next() },
            Op::Classalloc { name, zone } => Op::Classalloc { name, zone: zone.map(|_| next()) },
            Op::Fieldload { ty, name, .. } => Op::Fieldload { ty, obj: next(), name },
            Op::Field { name, .. } => Op::Field { obj: next(), name },
            Op::Fieldstore { ty, name, .. } => Op::Fieldstore { ty, obj: next(), name, value: next() },
            Op::Method { sig, .. } => Op::Method { obj: next(), sig },
            Op::Dynmethod { sig, .. } => Op::Dynmethod { obj: next(), sig },
            Op::As { ty, .. } => Op::As { ty, obj: next() },
            Op::Is { ty, .. } => Op::Is { ty, obj: next() },
            Op::Box { ty, .. } => Op::Box { ty, obj: next() },
            Op::Unbox { ty, .. } => Op::Unbox { ty, obj: next() },
            Op::Copy(_) => Op::Copy(next()),
            Op::Varload(_) => Op::Varload(next()),
            Op::Arraylength(_) => Op::Arraylength(next()),
            Op::Varstore { .. } => Op::Varstore { slot: next(), value: next() },
            Op::Arrayalloc { ty, zone, .. } => {
                let init = next();
                Op::Arrayalloc { ty, init, zone: zone.map(|_| next()) }
            }
            Op::Arrayload { ty, .. } => Op::Arrayload { ty, arr: next(), idx: next() },
            Op::Arraystore { ty, .. } => Op::Arraystore { ty, arr: next(), idx: next(), value: next() },
            op @ (Op::Fence(_) | Op::Module(_) | Op::SizeOf(_) | Op::AlignmentOf(_) | Op::Var(_)) => op,
        }
    }
}

impl Next {
    /// The values control passes along this edge.
    pub fn operands(&self) -> Vec<ValId> {
        match self {
            Next::None => Vec::new(),
            Next::Unwind(_, to) => to.operands(),
            Next::Case(v, to) => std::iter::once(*v).chain(to.operands()).collect(),
            Next::Label(_, args) => args.clone(),
        }
    }
}

impl Inst {
    /// Calls `f` with every value this instruction reads, as `operands` lists them.
    pub fn for_each_operand(&self, f: &mut impl FnMut(ValId)) {
        fn next(n: &Next, f: &mut impl FnMut(ValId)) {
            match n {
                Next::None => {}
                Next::Unwind(_, to) => next(to, f),
                Next::Case(v, to) => {
                    f(*v);
                    next(to, f);
                }
                Next::Label(_, args) => args.iter().for_each(|a| f(*a)),
            }
        }
        match self {
            Inst::Label(..) => {}
            Inst::Let { op, unwind, .. } => {
                op.for_each_operand(f);
                next(unwind, f);
            }
            Inst::Ret(v) => f(*v),
            Inst::Jump(n) | Inst::Unreachable(n) => next(n, f),
            Inst::If(c, t, e) => {
                f(*c);
                next(t, f);
                next(e, f);
            }
            Inst::Switch(v, d, cases) => {
                f(*v);
                next(d, f);
                cases.iter().for_each(|c| next(c, f));
            }
            Inst::Throw(v, n) => {
                f(*v);
                next(n, f);
            }
            Inst::LinktimeIf(_, t, e) => {
                next(t, f);
                next(e, f);
            }
        }
    }

    /// Every value this instruction reads, including those passed to other blocks.
    pub fn operands(&self) -> Vec<ValId> {
        match self {
            Inst::Label(..) => Vec::new(),
            Inst::Let { op, unwind, .. } => [op.operands(), unwind.operands()].concat(),
            Inst::Ret(v) => vec![*v],
            Inst::Jump(n) => n.operands(),
            Inst::If(c, t, e) => [vec![*c], t.operands(), e.operands()].concat(),
            Inst::Switch(v, d, cases) => {
                let mut out = vec![*v];
                out.extend(d.operands());
                cases.iter().for_each(|c| out.extend(c.operands()));
                out
            }
            Inst::Throw(v, n) => [vec![*v], n.operands()].concat(),
            Inst::Unreachable(n) => n.operands(),
            Inst::LinktimeIf(_, t, e) => [t.operands(), e.operands()].concat(),
        }
    }
}
