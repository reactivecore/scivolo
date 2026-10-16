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

impl Vm {
    /// Calls method `sig` of module `module` with `args`, `host` serving its quotes; the number
    /// of the expression it returns. `host` is handed back whatever the outcome.
    pub fn expand_macro(
        &mut self,
        host: Box<dyn QuoteHost>,
        (module, sig): (&str, &str),
        args: &[MacroArg],
    ) -> (Box<dyn QuoteHost>, Result<u32, VmError>) {
        self.quotes = Some(host);
        let (module, quote_host) = (self.world.top(module), self.world.top(HOST));
        let sig = StrId(self.world.pool.strings.intern(sig.into()));
        let [expr, tpe, quotes, id] = [
            "D4expriL17scala.quoted.ExprEO",
            "D3tpeiL17scala.quoted.TypeEO",
            "D6quotesL19scala.quoted.QuotesEO",
            "D6exprIdL17scala.quoted.ExpriEO",
        ]
        .map(|s| self.world.member(quote_host, s));
        let (args, out) = (args.to_vec(), Rc::new(Cell::new(0)));
        let result = out.clone();
        let run = self.run_main_thread(move |vm| {
            let host = vm.module(quote_host)? as u64;
            let target = vm.module(module)?;
            let mut values = vec![target as u64];
            for arg in args {
                values.push(match arg {
                    MacroArg::Expr(n) => vm.call_global(expr, &[host, n as u64])?,
                    MacroArg::Type(n) => vm.call_global(tpe, &[host, n as u64])?,
                    MacroArg::Quotes => vm.call_global(quotes, &[host])?,
                });
            }
            let f = vm.virtual_method(target, sig)?;
            let returned = vm.call(f, &values)?;
            result.set(vm.call_global(id, &[host, returned])? as u32);
            Ok(())
        });
        let host = self.quotes.take().expect("the quote host of the macro expansion");
        (host, run.map(|()| out.get()))
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
        (QUOTES, "D7reflectL33scala.quoted.Quotes$reflectModuleEO") => {
            |vm, _| vm.throw_new_with(UNSUPPORTED, "quotes.reflect")
        }
        _ => return None,
    };
    Some(f)
}

/// What `f` answers of the quote host, an `Unsupported` thrown for a refusal or outside a
/// macro expansion.
fn with_host(vm: &mut Vm, f: impl FnOnce(&mut dyn QuoteHost) -> Result<u32, &'static str>) -> Flow {
    let answer = match vm.quotes.as_deref_mut() {
        Some(host) => {
            catch_unwind(AssertUnwindSafe(|| f(host))).unwrap_or(Err("a quote the frontend fails on"))
        }
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
