//! The results of operations encoded into their answer.

use super::Fallible;
use super::decode::Expr;
use crate::host::Host;
use tasty::{SymbolId, TypeId};
use typer::tpd::TNodeId;
use vm::Reflected;

/// A result and how to add it to an answer.
pub trait Encode {
    fn put(self, h: &mut Host, out: &mut Reflected) -> Fallible<()>;
}

impl<T: Encode> Encode for Fallible<T> {
    fn put(self, h: &mut Host, out: &mut Reflected) -> Fallible<()> {
        self?.put(h, out)
    }
}

impl Encode for Reflected {
    fn put(self, _: &mut Host, out: &mut Reflected) -> Fallible<()> {
        out.0.extend(self.0);
        out.1 = self.1.or(out.1.take());
        Ok(())
    }
}

impl Encode for () {
    fn put(self, _: &mut Host, _: &mut Reflected) -> Fallible<()> {
        Ok(())
    }
}

impl Encode for i32 {
    fn put(self, _: &mut Host, out: &mut Reflected) -> Fallible<()> {
        out.0.push(self);
        Ok(())
    }
}

impl Encode for bool {
    fn put(self, h: &mut Host, out: &mut Reflected) -> Fallible<()> {
        (self as i32).put(h, out)
    }
}

/// The low and the high word.
impl Encode for u64 {
    fn put(self, _: &mut Host, out: &mut Reflected) -> Fallible<()> {
        out.0.extend([self as u32 as i32, (self >> 32) as u32 as i32]);
        Ok(())
    }
}

impl Encode for String {
    fn put(self, _: &mut Host, out: &mut Reflected) -> Fallible<()> {
        out.1 = Some(self);
        Ok(())
    }
}

impl Encode for SymbolId {
    fn put(self, h: &mut Host, out: &mut Reflected) -> Fallible<()> {
        (self.0 as i32).put(h, out)
    }
}

impl Encode for Option<SymbolId> {
    fn put(self, h: &mut Host, out: &mut Reflected) -> Fallible<()> {
        self.map_or(-1, |s| s.0 as i32).put(h, out)
    }
}

impl Encode for TypeId {
    fn put(self, h: &mut Host, out: &mut Reflected) -> Fallible<()> {
        (h.add_type(self) as i32).put(h, out)
    }
}

impl Encode for TNodeId {
    fn put(self, h: &mut Host, out: &mut Reflected) -> Fallible<()> {
        (h.add_tree(self) as i32).put(h, out)
    }
}

impl Encode for Option<TNodeId> {
    fn put(self, h: &mut Host, out: &mut Reflected) -> Fallible<()> {
        match self {
            Some(t) => t.put(h, out),
            None => (-1).put(h, out),
        }
    }
}

impl Encode for Expr {
    fn put(self, h: &mut Host, out: &mut Reflected) -> Fallible<()> {
        self.number.put(h, out)
    }
}

impl<T: Encode> Encode for Vec<T> {
    fn put(self, h: &mut Host, out: &mut Reflected) -> Fallible<()> {
        self.into_iter().try_for_each(|x| x.put(h, out))
    }
}

impl<T: Encode, const N: usize> Encode for [T; N] {
    fn put(self, h: &mut Host, out: &mut Reflected) -> Fallible<()> {
        self.into_iter().try_for_each(|x| x.put(h, out))
    }
}
