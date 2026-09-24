use super::Reader;
use crate::cursor::Result;
use crate::nir::{BINS, COMPS, CONVS, Cond, Inst, MemoryOrder, Next, ORDERS, Op, Param, PosId};
use crate::tags::{cond, inst, next, op};

impl Reader<'_> {
    pub fn inst(&mut self) -> Result<(Inst, PosId)> {
        let tag = self.tag()?;
        let pos = self.position()?;
        let inst = match tag {
            inst::LABEL => Inst::Label(self.local()?, self.seq(Self::param)?),
            inst::LET => {
                Inst::Let { id: self.local()?, op: self.op()?, unwind: Next::None, scope: self.c.uleb32()? }
            }
            inst::LET_UNWIND => {
                Inst::Let { id: self.local()?, op: self.op()?, unwind: self.next()?, scope: self.c.uleb32()? }
            }
            inst::RET => Inst::Ret(self.val()?),
            inst::JUMP => Inst::Jump(self.next()?),
            inst::IF => Inst::If(self.val()?, self.next()?, self.next()?),
            inst::SWITCH => Inst::Switch(self.val()?, self.next()?, self.seq(Self::next)?),
            inst::THROW => Inst::Throw(self.val()?, self.next()?),
            inst::UNREACHABLE => Inst::Unreachable(self.next()?),
            inst::LINKTIME_IF => Inst::LinktimeIf(Box::new(self.cond()?), self.next()?, self.next()?),
            t => return self.bad_tag("instruction", t),
        };
        Ok((inst, pos))
    }

    fn param(&mut self) -> Result<Param> {
        Ok(Param { id: self.local()?, ty: self.ty()? })
    }

    fn next(&mut self) -> Result<Next> {
        self.nested(|r| {
            Ok(match r.tag()? {
                next::NONE => Next::None,
                next::UNWIND => Next::Unwind(r.param()?, Box::new(r.next()?)),
                next::CASE => Next::Case(r.val()?, Box::new(r.next()?)),
                next::LABEL => Next::Label(r.local()?, r.vals()?),
                t => return r.bad_tag("next", t),
            })
        })
    }

    fn cond(&mut self) -> Result<Cond> {
        self.nested(Self::cond_node)
    }

    fn cond_node(&mut self) -> Result<Cond> {
        Ok(match self.tag()? {
            cond::SIMPLE => Cond::Simple {
                property: self.string()?,
                comp: self.table(&COMPS, "comparison")?,
                value: self.val()?,
                pos: self.position()?,
            },
            cond::COMPLEX => Cond::Complex {
                bin: self.table(&BINS, "binary op")?,
                left: Box::new(self.cond()?),
                right: Box::new(self.cond()?),
                pos: self.position()?,
            },
            t => return self.bad_tag("linktime condition", t),
        })
    }

    fn table<T: Copy>(&mut self, table: &[T], what: &str) -> Result<T> {
        let tag = self.tag()?;
        match table.get((tag as usize).wrapping_sub(1)) {
            Some(&t) => Ok(t),
            None => self.bad_tag(what, tag),
        }
    }

    fn order(&mut self) -> Result<MemoryOrder> {
        self.table(&ORDERS, "memory order")
    }

    fn indexes(&mut self) -> Result<Vec<i32>> {
        self.seq(|r| r.c.sleb32())
    }

    fn op(&mut self) -> Result<Op> {
        Ok(match self.tag()? {
            op::CALL => Op::Call { ty: self.ty()?, ptr: self.val()?, args: self.vals()? },
            op::LOAD => Op::Load { ty: self.ty()?, ptr: self.val()?, order: None },
            op::LOAD_ATOMIC => Op::Load { ty: self.ty()?, ptr: self.val()?, order: Some(self.order()?) },
            op::STORE => Op::Store { ty: self.ty()?, ptr: self.val()?, value: self.val()?, order: None },
            op::STORE_ATOMIC => {
                Op::Store { ty: self.ty()?, ptr: self.val()?, value: self.val()?, order: Some(self.order()?) }
            }
            op::ELEM => Op::Elem { ty: self.ty()?, ptr: self.val()?, indexes: self.vals()? },
            op::EXTRACT => Op::Extract { aggr: self.val()?, indexes: self.indexes()? },
            op::INSERT => Op::Insert { aggr: self.val()?, value: self.val()?, indexes: self.indexes()? },
            op::STACKALLOC => Op::Stackalloc { ty: self.ty()?, n: self.val()? },
            op::BIN => Op::Bin {
                bin: self.table(&BINS, "binary op")?,
                ty: self.ty()?,
                l: self.val()?,
                r: self.val()?,
            },
            op::COMP => Op::Comp {
                comp: self.table(&COMPS, "comparison")?,
                ty: self.ty()?,
                l: self.val()?,
                r: self.val()?,
            },
            op::CONV => {
                Op::Conv { conv: self.table(&CONVS, "conversion")?, ty: self.ty()?, value: self.val()? }
            }
            op::FENCE => Op::Fence(self.order()?),
            op::CLASSALLOC => Op::Classalloc { name: self.global()?, zone: None },
            op::CLASSALLOC_ZONE => Op::Classalloc { name: self.global()?, zone: Some(self.val()?) },
            op::FIELDLOAD => Op::Fieldload { ty: self.ty()?, obj: self.val()?, name: self.global()? },
            op::FIELDSTORE => {
                Op::Fieldstore { ty: self.ty()?, obj: self.val()?, name: self.global()?, value: self.val()? }
            }
            op::FIELD => Op::Field { obj: self.val()?, name: self.global()? },
            op::METHOD => Op::Method { obj: self.val()?, sig: self.string()? },
            op::DYNMETHOD => Op::Dynmethod { obj: self.val()?, sig: self.string()? },
            op::MODULE => Op::Module(self.global()?),
            op::AS => Op::As { ty: self.ty()?, obj: self.val()? },
            op::IS => Op::Is { ty: self.ty()?, obj: self.val()? },
            op::COPY => Op::Copy(self.val()?),
            op::BOX => Op::Box { ty: self.ty()?, obj: self.val()? },
            op::UNBOX => Op::Unbox { ty: self.ty()?, obj: self.val()? },
            op::VAR => Op::Var(self.ty()?),
            op::VARLOAD => Op::Varload(self.val()?),
            op::VARSTORE => Op::Varstore { slot: self.val()?, value: self.val()? },
            op::ARRAYALLOC => Op::Arrayalloc { ty: self.ty()?, init: self.val()?, zone: None },
            op::ARRAYALLOC_ZONE => {
                Op::Arrayalloc { ty: self.ty()?, init: self.val()?, zone: Some(self.val()?) }
            }
            op::ARRAYLOAD => Op::Arrayload { ty: self.ty()?, arr: self.val()?, idx: self.val()? },
            op::ARRAYSTORE => {
                Op::Arraystore { ty: self.ty()?, arr: self.val()?, idx: self.val()?, value: self.val()? }
            }
            op::ARRAYLENGTH => Op::Arraylength(self.val()?),
            op::SIZE_OF => Op::SizeOf(self.ty()?),
            op::ALIGNMENT_OF => Op::AlignmentOf(self.ty()?),
            t => return self.bad_tag("op", t),
        })
    }
}
