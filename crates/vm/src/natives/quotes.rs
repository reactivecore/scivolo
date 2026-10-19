//! Macro expansions: a macro implementation called with `Expr`s, `Type`s and a `Quotes` that
//! stand for the frontend's trees and types, and the natives of `QuoteHost` in the overlay that
//! reach them (ADR 0029).

use super::NativeFn;
use crate::interp::Flow;
use crate::layout::{ARRAY_HEADER, ARRAY_LENGTH};
use crate::{Vm, VmError, heap};
use nir::StrId;
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

const HOST: &str = "scala.quoted.runtime.impl.QuoteHost$";
const QUOTES: &str = "scala.quoted.runtime.impl.QuotesImpl";
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
    /// Refuses the expansion for `what`, whatever the macro returns.
    fn refuse(&mut self, what: &str);
}

/// A term hole of a quote: the index its pickle gives it and its arguments, an expression
/// `Ok`, a type `Err`.
pub type Hole = (u32, Vec<Result<u32, u32>>);

/// An argument of a macro implementation.
#[derive(Clone, Copy)]
pub enum MacroArg {
    Expr(u32),
    Type(u32),
    Quotes,
}

/// The macro expansions of a VM: the one running, if one is, with its host and arguments, and
/// how many there were.
#[derive(Default)]
pub struct Expansions {
    running: Option<(Box<dyn QuoteHost>, Vec<MacroArg>)>,
    count: u32,
}

impl Vm {
    /// Calls method `sig` of module `module` with `args`, `host` serving its quotes; the number
    /// of the expression it returns. `host` is handed back whatever the outcome.
    pub fn expand_macro(
        &mut self,
        host: Box<dyn QuoteHost>,
        (module, sig): (&str, &str),
        args: &[MacroArg],
    ) -> (Box<dyn QuoteHost>, Result<u32, VmError>) {
        self.quotes.running = Some((host, args.to_vec()));
        self.quotes.count += 1;
        let (module, quote_host) = (self.world.top(module), self.world.top(HOST));
        let sig = StrId(self.world.pool.strings.intern(sig.into()));
        let [made_args, id] = ["D4argsLAL16java.lang.Object_EO", "D6exprIdL17scala.quoted.ExpriEO"]
            .map(|s| self.world.member(quote_host, s));
        let out = Rc::new(Cell::new(0));
        let result = out.clone();
        let run = self.run_main_thread(move |vm| {
            let host = vm.module(quote_host)? as u64;
            let target = vm.module(module)?;
            let f = vm.virtual_method(target, sig)?;
            let arr = vm.call_global(made_args, &[host])? as usize;
            let len: i32 = heap::read(arr + ARRAY_LENGTH);
            let at = |i: usize| heap::read::<u64>(arr + ARRAY_HEADER as usize + 8 * i);
            let values: Vec<u64> = std::iter::once(target as u64).chain((0..len as usize).map(at)).collect();
            let returned = vm.call(f, &values)?;
            std::hint::black_box(arr);
            result.set(vm.call_global(id, &[host, returned])? as u32);
            Ok(())
        });
        let (host, _) = self.quotes.running.take().expect("the quote host of the macro expansion");
        (host, run.map(|()| out.get()))
    }

    /// How many macro expansions ran.
    pub fn macro_expansions(&self) -> u32 {
        self.quotes.count
    }
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
            let arg = vm.quotes.running.as_ref().and_then(|(_, a)| a.get(args[1] as usize).copied());
            Ok(match arg {
                Some(MacroArg::Expr(n)) => n as u64,
                Some(MacroArg::Type(n)) => !n as u64,
                Some(MacroArg::Quotes) | None => i32::MIN as u32 as u64,
            })
        },
        (HOST, "D6refuseL16java.lang.StringuEO") => |vm, args| {
            let what = if args[1] == 0 { String::new() } else { vm.read_string(args[1] as usize)? };
            if let Some((host, _)) = vm.quotes.running.as_mut() {
                host.refuse(&what);
            }
            Ok(0)
        },
        (QUOTES, "D7reflectL33scala.quoted.Quotes$reflectModuleEO") => {
            |vm, _| vm.throw_new_with(UNSUPPORTED, "quotes.reflect")
        }
        _ => return None,
    };
    Some(f)
}

/// What `f` answers of the quote host, an `Unsupported` thrown for a refusal, a panic or
/// outside a macro expansion.
fn with_host(vm: &mut Vm, f: impl FnOnce(&mut dyn QuoteHost) -> Result<u32, &'static str>) -> Flow {
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

fn unpickle(vm: &mut Vm, args: &[u64], term: bool) -> Flow {
    if args[1] == 0 || args[2] == 0 {
        return vm.null_pointer();
    }
    let pickled = vm.read_string(args[1] as usize)?;
    let arr = args[2] as usize;
    let len: i32 = heap::read(arr + ARRAY_LENGTH);
    let types: Vec<u32> =
        (0..len as usize).map(|i| heap::read::<i32>(arr + ARRAY_HEADER as usize + 4 * i) as u32).collect();
    with_host(vm, |h| match term {
        true => h.unpickle_expr(&pickled, &types),
        false => h.unpickle_type(&pickled, &types),
    })
}
