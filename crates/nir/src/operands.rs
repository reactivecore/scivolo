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
