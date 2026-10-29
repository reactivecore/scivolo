//! The arguments of operations decoded from their numbers and text.

use super::Fallible;
use crate::host::Host;
use std::marker::PhantomData;
use tasty::{SymbolId, TypeId};
use typer::tpd::TNodeId;

/// The numbers and the text of an operation, the numbers from `at` on not yet decoded.
pub struct Args<'a> {
    numbers: &'a [i32],
    at: usize,
    text: &'a str,
}

impl<'a> Args<'a> {
    pub fn new(numbers: &'a [i32], text: &'a str) -> Self {
        Args { numbers, at: 0, text }
    }

    fn next(&mut self) -> Fallible<i32> {
        let n = self.numbers.get(self.at).copied().ok_or(NO_ARGUMENTS)?;
        self.at += 1;
        Ok(n)
    }
}

/// The refusal of an operation with fewer numbers than it takes.
pub const NO_ARGUMENTS: &str = "a quotes.reflect operation without its arguments";

/// A kind of argument and how to decode it.
pub trait Decode {
    type Out;
    fn decode(h: &Host, args: &mut Args<'_>) -> Fallible<Self::Out>;
}

/// A number as it is.
pub struct Num;
/// Two numbers as the low and the high word.
pub struct Bits;
/// A symbol number, refused for `Symbol.noSymbol`.
pub struct Sym;
/// A symbol number, `None` for `Symbol.noSymbol`.
pub struct OptSym;
/// A type number.
pub struct Ty;
/// A type number of a type test or extractor.
pub struct TyShape;
/// An expression number.
pub struct Tree;
/// An expression number, `None` for a negative one.
pub struct OptTree;
/// The text.
pub struct Name;
/// The arguments of kind `K` from here on.
pub struct Rest<K>(PhantomData<K>);

/// An expression number and its tree.
#[derive(Clone, Copy)]
pub struct Expr {
    pub number: i32,
    pub tree: TNodeId,
}

impl Decode for Num {
    type Out = i32;
    fn decode(_: &Host, args: &mut Args<'_>) -> Fallible<i32> {
        args.next()
    }
}

impl Decode for Bits {
    type Out = u64;
    fn decode(_: &Host, args: &mut Args<'_>) -> Fallible<u64> {
        Ok((args.next()? as u32 as u64) | ((args.next()? as u32 as u64) << 32))
    }
}

impl Decode for Sym {
    type Out = SymbolId;
    fn decode(_: &Host, args: &mut Args<'_>) -> Fallible<SymbolId> {
        symbol(args.next()?)
    }
}

impl Decode for OptSym {
    type Out = Option<SymbolId>;
    fn decode(_: &Host, args: &mut Args<'_>) -> Fallible<Option<SymbolId>> {
        Ok(symbol(args.next()?).ok())
    }
}

impl Decode for Ty {
    type Out = TypeId;
    fn decode(h: &Host, args: &mut Args<'_>) -> Fallible<TypeId> {
        h.type_of(args.next()? as u32)
    }
}

impl Decode for TyShape {
    type Out = TypeId;
    fn decode(h: &Host, args: &mut Args<'_>) -> Fallible<TypeId> {
        h.type_shape_of(args.next()? as u32)
    }
}

impl Decode for Tree {
    type Out = TNodeId;
    fn decode(h: &Host, args: &mut Args<'_>) -> Fallible<TNodeId> {
        h.tree(args.next()? as u32)
    }
}

impl Decode for OptTree {
    type Out = Option<TNodeId>;
    fn decode(h: &Host, args: &mut Args<'_>) -> Fallible<Option<TNodeId>> {
        let n = args.next()?;
        (n >= 0).then(|| h.tree(n as u32)).transpose()
    }
}

impl Decode for Expr {
    type Out = Expr;
    fn decode(h: &Host, args: &mut Args<'_>) -> Fallible<Expr> {
        let number = args.next()?;
        Ok(Expr { number, tree: h.tree(number as u32)? })
    }
}

impl Decode for Name {
    type Out = String;
    fn decode(_: &Host, args: &mut Args<'_>) -> Fallible<String> {
        Ok(args.text.to_owned())
    }
}

impl<K: Decode> Decode for Rest<K> {
    type Out = Vec<K::Out>;
    fn decode(h: &Host, args: &mut Args<'_>) -> Fallible<Vec<K::Out>> {
        let mut rest = Vec::new();
        while args.at < args.numbers.len() {
            rest.push(K::decode(h, args)?);
        }
        Ok(rest)
    }
}

impl<A: Decode, B: Decode> Decode for (A, B) {
    type Out = (A::Out, B::Out);
    fn decode(h: &Host, args: &mut Args<'_>) -> Fallible<Self::Out> {
        Ok((A::decode(h, args)?, B::decode(h, args)?))
    }
}

/// Symbol number `n`.
fn symbol(n: i32) -> Fallible<SymbolId> {
    (n >= 0).then_some(SymbolId(n as u32)).ok_or("a quotes.reflect operation on Symbol.noSymbol")
}
