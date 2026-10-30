//! Runs of macro implementations and macro annotations, `host` serving their quotes.

use super::quotes::{HOST, MacroArg, QuoteHost, after};
use crate::interp::Flow;
use crate::layout::{ARRAY_HEADER, ARRAY_LENGTH};
use crate::{Vm, VmError, heap};
use nir::StrId;
use std::cell::Cell;
use std::rc::Rc;

const ANNOTATE: &str = "D8annotateL16java.lang.ObjectiiiEO";

impl Vm {
    /// Calls method `sig` of module `module` with `args`, `host` serving its quotes; the number
    /// of the expression it returns. `host` is handed back whatever the outcome.
    pub fn expand_macro(
        &mut self,
        host: Box<dyn QuoteHost>,
        (module, sig): (&str, &str),
        args: &[MacroArg],
    ) -> (Box<dyn QuoteHost>, Result<u32, VmError>) {
        let module = self.world.top(module);
        let sig = StrId(self.world.pool.strings.intern(sig.into()));
        let quote_host = self.world.top(HOST);
        let id = self.world.member(quote_host, "D6exprIdL17scala.quoted.ExpriEO");
        self.expansion(host, args, move |vm, quote_host, values| {
            let target = vm.module(module)?;
            let f = vm.virtual_method(target, sig)?;
            let returned = vm.call(f, &[&[target as u64], values].concat())?;
            Ok(vm.call_global(id, &[quote_host, returned])? as u32)
        })
    }

    /// Calls `transform` of a new instance of macro annotation `class`, made by constructor
    /// `ctor` from `args`, on expressions `definition` and `companion` (`-1` for none) through
    /// `QuoteHost.annotate`, `host` serving its quotes. `host` is handed back whatever the outcome.
    pub fn expand_annotation(
        &mut self,
        host: Box<dyn QuoteHost>,
        (class, ctor): (&str, &str),
        args: &[MacroArg],
        (definition, companion): (u32, i32),
    ) -> (Box<dyn QuoteHost>, Result<u32, VmError>) {
        let class = self.world.top(class);
        let ctor = self.world.member(class, ctor);
        let quote_host = self.world.top(HOST);
        let annotate = self.world.member(quote_host, ANNOTATE);
        self.expansion(host, args, move |vm, quote_host, values| {
            let cls = vm.link(class)?;
            let annotation = vm.alloc_object(cls) as u64;
            vm.call_global(ctor, &[&[annotation], values].concat())?;
            let made = [quote_host, annotation, definition as u64, companion as u32 as u64];
            let n = vm.call_global(annotate, &made)?;
            std::hint::black_box(annotation);
            Ok(n as u32)
        })
    }

    /// What `call` answers on the `QuoteHost` module and the values of `args` in a run of the main
    /// thread, `host` serving its quotes.
    fn expansion(
        &mut self,
        host: Box<dyn QuoteHost>,
        args: &[MacroArg],
        call: impl FnOnce(&mut Vm, u64, &[u64]) -> Flow<u32> + 'static,
    ) -> (Box<dyn QuoteHost>, Result<u32, VmError>) {
        let unboxed: Vec<Option<u64>> = std::iter::successors(Some(0), |&i| Some(after(args, i)))
            .take_while(|&i| i < args.len())
            .map(|i| match args[i] {
                MacroArg::Unboxed(bits) => Some(bits),
                _ => None,
            })
            .collect();
        self.quotes.running = Some((host, args.to_vec()));
        self.quotes.count += 1;
        let quote_host = self.world.top(HOST);
        let made_args = self.world.member(quote_host, "D4argsLAL16java.lang.Object_EO");
        let out = Rc::new(Cell::new(0));
        let result = out.clone();
        let run = self.run_main_thread(move |vm| {
            let host = vm.module(quote_host)? as u64;
            let arr = vm.call_global(made_args, &[host])? as usize;
            let len: i32 = heap::read(arr + ARRAY_LENGTH);
            let at = |i: usize| match unboxed.get(i) {
                Some(&Some(bits)) => bits,
                _ => heap::read::<u64>(arr + ARRAY_HEADER as usize + 8 * i),
            };
            let values: Vec<u64> = (0..len as usize).map(at).collect();
            result.set(call(vm, host, &values)?);
            std::hint::black_box(arr);
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
