use crate::{Inst, Local, Next};

impl Next {
    /// The block this successor enters, if any.
    pub fn label(&self) -> Option<Local> {
        match self {
            Next::Label(l, _) => Some(*l),
            Next::Unwind(_, to) | Next::Case(_, to) => to.label(),
            Next::None => None,
        }
    }
}

impl Inst {
    /// Calls `f` with each block this instruction may continue at, handlers included.
    pub fn for_each_successor(&self, f: &mut impl FnMut(Local)) {
        let mut next = |n: &Next| n.label().into_iter().for_each(&mut *f);
        match self {
            Inst::Let { unwind: n, .. } | Inst::Throw(_, n) | Inst::Unreachable(n) | Inst::Jump(n) => next(n),
            Inst::If(_, t, e) | Inst::LinktimeIf(_, t, e) => {
                next(t);
                next(e);
            }
            Inst::Switch(_, d, cases) => {
                next(d);
                cases.iter().for_each(next);
            }
            Inst::Label(..) | Inst::Ret(_) => {}
        }
    }
}
