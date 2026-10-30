//! Macro expansions: a macro implementation called with the frontend's trees and types as
//! `Expr`s and `Type`s, and the natives of `QuoteHost` in the overlay that reach them (ADR 0029).

use super::NativeFn;
use crate::interp::Flow;
use crate::layout::{ARRAY_HEADER, ARRAY_LENGTH};
use crate::{Vm, heap};
use std::panic::{AssertUnwindSafe, catch_unwind};

pub const HOST: &str = "scala.quoted.runtime.impl.QuoteHost$";
const UNSUPPORTED: &str = "scala.quoted.runtime.impl.Unsupported";

/// The frontend's side of the macro expansion running: expressions and types by number.
pub trait QuoteHost: std::any::Any {
    /// The expression pickled as `pickled`, its type holes filled with types `types`, its term
    /// holes open.
    fn unpickle_expr(&mut self, pickled: &str, types: &[u32]) -> Result<u32, &'static str>;
    /// The type pickled as `pickled`, its type holes filled with types `types`.
    fn unpickle_type(&mut self, pickled: &str, types: &[u32]) -> Result<u32, &'static str>;
    fn hole_count(&self, quote: u32) -> u32;
    fn hole(&self, quote: u32, hole: u32) -> Option<Hole>;
    fn fill_hole(&mut self, quote: u32, hole: u32, expr: u32) -> Result<(), &'static str>;
    /// Matches expression `scrutinee` against quote pattern `pattern`: how many things it
    /// matched, `u32::MAX` if it does not match.
    fn match_pattern(&mut self, scrutinee: u32, pattern: u32) -> Result<u32, &'static str>;
    /// Thing `i` the last match matched: an expression `Ok`, a type `Err`.
    fn matched(&self, i: u32) -> Option<Result<u32, u32>>;
    /// Operation `op` of `quotes.reflect` on handles and numbers `args` and text `text`: its
    /// handles and numbers, and its text if it answers one.
    fn reflect(&mut self, op: &str, args: &[i32], text: &str) -> Result<Reflected, &'static str>;
    /// Refuses the expansion for `what`, whatever the macro returns.
    fn refuse(&mut self, what: &str);
}

/// A term hole of a quote: the index its pickle gives it and its arguments, an expression
/// `Ok`, a type `Err`.
pub type Hole = (u32, Vec<Result<u32, u32>>);

/// What a `quotes.reflect` operation answers: numbers and a text.
pub type Reflected = (Vec<i32>, Option<String>);

/// An argument of a macro implementation; a literal boxed by the JVM descriptor letter of its
/// type (`T` a string, `N` null), its value's bits and its text, or a primitive's bits unboxed.
/// The arguments after `Varargs(n)` are the `n` elements of a `Seq`, the one after `ByName` the
/// value of a `Function0`.
#[derive(Clone)]
pub enum MacroArg {
    Expr(u32),
    Type(u32),
    Quotes,
    Literal(u8, u64, Option<String>),
    Unboxed(u64),
    Varargs(u32),
    ByName,
}

/// The index of the argument after argument `i` of `args` and the arguments it holds.
pub fn after(args: &[MacroArg], i: usize) -> usize {
    match args.get(i) {
        Some(&MacroArg::Varargs(n)) => (0..n).fold(i + 1, |j, _| after(args, j)),
        Some(MacroArg::ByName) => after(args, i + 1),
        _ => i + 1,
    }
}

/// The macro expansions of a VM: the one running, if one is, with its host and arguments, and
/// how many there were.
#[derive(Default)]
pub struct Expansions {
    pub running: Option<(Box<dyn QuoteHost>, Vec<MacroArg>)>,
    pub count: u32,
    /// What the last `quotes.reflect` operation answered.
    pub reflected: Reflected,
}

pub fn method(owner: &str, sig: &str) -> Option<NativeFn> {
    let f: NativeFn = match (owner, sig) {
        (HOST, "D12unpickleExprL16java.lang.StringLAi_iEO") => |vm, args| unpickle(vm, args, true),
        (HOST, "D12unpickleTypeL16java.lang.StringLAi_iEO") => |vm, args| unpickle(vm, args, false),
        (HOST, "D9holeCountiiEO") => |vm, args| with_host(vm, |h| Ok(h.hole_count(args[1] as u32))),
        (HOST, "D9holeIndexiiiEO") => |vm, args| hole(vm, args, |(index, _), _| Some(index)),
        (HOST, "D12holeArgCountiiiEO") => |vm, args| hole(vm, args, |(_, a), _| Some(a.len() as u32)),
        (HOST, "D7holeArgiiiiEO") => |vm, args| {
            let arg = |(_, a): Hole, i: u64| match *a.get(i as usize)? {
                Ok(e) => Some(e),
                Err(t) => Some(!t),
            };
            hole(vm, args, arg)
        },
        (HOST, "D8fillHoleiiiuEO") => |vm, args| {
            with_host(vm, |h| h.fill_hole(args[1] as u32, args[2] as u32, args[3] as u32).map(|()| 0))
        },
        (HOST, "D9expansioniEO") => |vm, _| Ok(vm.quotes.count as u64),
        (HOST, "D8argCountiEO") => |vm, _| Ok(vm.quotes.running.as_ref().map_or(0, |(_, a)| a.len() as u64)),
        (HOST, "D3argiiEO") => |vm, args| {
            Ok(match macro_arg(vm, args[1]) {
                Some(&MacroArg::Expr(n)) => n as u64,
                Some(&MacroArg::Type(n)) => !n as u64,
                Some(MacroArg::Quotes) | None => i32::MIN as u32 as u64,
                Some(MacroArg::Literal(..) | MacroArg::Unboxed(_)) => (i32::MIN + 1) as u32 as u64,
                Some(MacroArg::Varargs(_)) => (i32::MIN + 2) as u32 as u64,
                Some(MacroArg::ByName) => (i32::MIN + 3) as u32 as u64,
            })
        },
        (HOST, "D12elementCountiiEO") => |vm, args| match macro_arg(vm, args[1]) {
            Some(&MacroArg::Varargs(n)) => Ok(n as u64),
            _ => Ok(0),
        },
        (HOST, "D10literalTagiiEO") => |vm, args| match macro_arg(vm, args[1]) {
            Some(&MacroArg::Literal(tag, ..)) => Ok(tag as u64),
            _ => Ok(0),
        },
        (HOST, "D11literalBitsijEO") => |vm, args| match macro_arg(vm, args[1]) {
            Some(&MacroArg::Literal(_, bits, _)) => Ok(bits),
            _ => Ok(0),
        },
        (HOST, "D11literalTextiL16java.lang.StringEO") => |vm, args| match macro_arg(vm, args[1]).cloned() {
            Some(MacroArg::Literal(_, _, Some(text))) => Ok(vm.new_string(&text)? as u64),
            _ => Ok(0),
        },
        (HOST, "D6refuseL16java.lang.StringuEO") => |vm, args| {
            let what = if args[1] == 0 { String::new() } else { vm.read_string(args[1] as usize)? };
            if let Some((host, _)) = vm.quotes.running.as_mut() {
                host.refuse(&what);
            }
            Ok(0)
        },
        _ => return super::reflect::method(owner, sig),
    };
    Some(f)
}

/// Argument `i` of the macro expansion running.
pub fn macro_arg(vm: &Vm, i: u64) -> Option<&MacroArg> {
    vm.quotes.running.as_ref().and_then(|(_, a)| a.get(i as usize))
}

/// What `f` answers of the quote host; `Unsupported` thrown for a refusal, panic or no expansion.
pub fn with_host(vm: &mut Vm, f: impl FnOnce(&mut dyn QuoteHost) -> Result<u32, &'static str>) -> Flow {
    let answer = match vm.quotes.running.as_mut() {
        Some((host, _)) => catch_unwind(AssertUnwindSafe(|| f(host.as_mut())))
            .unwrap_or(Err("a quote the frontend fails on")),
        None => Err("QuoteHost outside a macro expansion"),
    };
    match answer {
        Ok(n) => Ok(n as u64),
        Err(why) => vm.throw_new_with(UNSUPPORTED, why),
    }
}

/// `pick(hole, args[3])` of hole `args[2]` of quote `args[1]`.
fn hole(vm: &mut Vm, args: &[u64], pick: fn(Hole, u64) -> Option<u32>) -> Flow {
    let (quote, hole, i) = (args[1] as u32, args[2] as u32, args.get(3).copied().unwrap_or(0));
    with_host(vm, |h| h.hole(quote, hole).and_then(|found| pick(found, i)).ok_or("no such quote hole"))
}

/// The elements of the `Array[Int]` at `arr`.
pub fn ints(arr: usize) -> Vec<i32> {
    let len: i32 = heap::read(arr + ARRAY_LENGTH);
    (0..len as usize).map(|i| heap::read(arr + ARRAY_HEADER as usize + 4 * i)).collect()
}

fn unpickle(vm: &mut Vm, args: &[u64], term: bool) -> Flow {
    if args[1] == 0 || args[2] == 0 {
        return vm.null_pointer();
    }
    let pickled = vm.read_string(args[1] as usize)?;
    let types: Vec<u32> = ints(args[2] as usize).into_iter().map(|t| t as u32).collect();
    with_host(vm, |h| match term {
        true => h.unpickle_expr(&pickled, &types),
        false => h.unpickle_type(&pickled, &types),
    })
}
