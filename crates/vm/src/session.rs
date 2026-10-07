//! Running Scala code: a whole program, or a session whose runtime lives across many runs, with
//! NIR added between them.

use crate::interp::{self, Flow};
use crate::{Vm, VmError};
use std::cell::Cell;
use std::rc::Rc;

const RUNTIME: &str = "scala.scalanative.runtime.package$";
const SHOW: &str = "scalai.repl.Show$";

impl Vm {
    /// Runs `main(args)` of the top-level object or class `entry` on the main green thread as a
    /// Scala Native executable does: runtime initialization, `main`, then the runtime's
    /// shutdown, which runs queued tasks. Interpreted calls throw `StackOverflowError` within
    /// `STACK_RESERVE` of the end of their thread's stack.
    pub fn run_main(&mut self, entry: &str, args: &[String]) -> Result<(), VmError> {
        let argv = self.c_argv(entry, args);
        let (entry, argc) = (entry.to_owned(), args.len() + 1);
        self.run_main_thread(move |vm| {
            let args = vm.init_runtime(argc, argv)?;
            let top = vm.world.top(&entry);
            let main = vm.world.member(top, "D4mainLAL16java.lang.String_uEo");
            vm.call_global(main, &[args])?;
            vm.shutdown_runtime()
        })
    }

    /// Initializes the runtime for program `program` with arguments `args`.
    pub fn start_runtime(&mut self, program: &str, args: &[String]) -> Result<(), VmError> {
        let argv = self.c_argv(program, args);
        let argc = args.len() + 1;
        self.run_main_thread(move |vm| vm.init_runtime(argc, argv).map(drop))
    }

    /// Adds NIR file `data` defining top-level `top`, which must not have been looked up yet.
    pub fn add_nir(&mut self, top: &str, data: Vec<u8>) -> Result<(), VmError> {
        Ok(self.world.add_file(top, data)?)
    }

    /// Constructs module `name` unless it was constructed before, also unsuccessfully.
    pub fn eval_module(&mut self, name: &str) -> Result<(), VmError> {
        let top = self.world.top(name);
        self.run_main_thread(move |vm| vm.module(top).map(drop))
    }

    /// What the no-argument method `method` of module `module`, returning a `java.lang.String`,
    /// returns.
    pub fn call_string(&mut self, module: &str, method: &str) -> Result<String, VmError> {
        let top = self.world.top(module);
        let mut sig = String::from("D");
        nir::sig::ident(&mut sig, method);
        sig.push_str("L16java.lang.StringEO");
        let f = self.world.member(top, &sig);
        let out = Rc::new(Cell::new(String::new()));
        let result = out.clone();
        self.run_main_thread(move |vm| {
            let obj = vm.module(top)? as u64;
            let s = vm.call_global(f, &[obj])?;
            result.set(vm.read_string(s as usize)?);
            Ok(())
        })?;
        Ok(out.take())
    }

    /// What the no-argument method `method` of module `module`, returning a `java.lang.Object`,
    /// returns, as the REPL prints it: `scalai.repl.Show(value, width, height, offset)`.
    pub fn show_value(
        &mut self,
        module: &str,
        method: &str,
        [width, height, offset]: [i32; 3],
    ) -> Result<String, VmError> {
        let top = self.world.top(module);
        let mut sig = String::from("D");
        nir::sig::ident(&mut sig, method);
        sig.push_str("L16java.lang.ObjectEO");
        let f = self.world.member(top, &sig);
        let show = self.world.top(SHOW);
        let apply = self.world.member(show, "D5applyL16java.lang.ObjectiiiL16java.lang.StringEO");
        let out = Rc::new(Cell::new(String::new()));
        let result = out.clone();
        self.run_main_thread(move |vm| {
            let show = vm.module(show)? as u64;
            let obj = vm.module(top)? as u64;
            let value = vm.call_global(f, &[obj])?;
            let s = vm.call_global(apply, &[show, value, width as u64, height as u64, offset as u64])?;
            result.set(vm.read_string(s as usize)?);
            Ok(())
        })?;
        Ok(out.take())
    }

    /// Shuts the runtime down, which runs queued tasks.
    pub fn shutdown(&mut self) -> Result<(), VmError> {
        self.run_main_thread(|vm| vm.shutdown_runtime())
    }

    /// Runs `body` on a new main green thread until it ends; an exception it does not catch is
    /// `VmError::Uncaught`.
    fn run_main_thread(&mut self, body: impl FnOnce(&mut Vm) -> Flow<()> + 'static) -> Result<(), VmError> {
        let vm: *mut Vm = self;
        let _running = self.callbacks.enter(vm);
        let main = move |vm: &mut Vm| {
            body(vm).map_err(|u| match u.thrown() {
                Some(exc) => VmError::Uncaught(vm.thrown(exc)).into(),
                None => u,
            })
        };
        match self.run_threads(main) {
            Ok(()) => Ok(()),
            Err(u) => match u.into_inner() {
                interp::Unwound::Throw(exc) => Err(VmError::Uncaught(self.thrown(exc))),
                interp::Unwound::Fatal(e) => Err(e),
            },
        }
    }

    /// The program's arguments array after initializing the runtime with C `argc` and `argv`.
    fn init_runtime(&mut self, argc: usize, argv: usize) -> Flow {
        let runtime = self.world.top(RUNTIME);
        let module = self.module(runtime)? as u64;
        let init = self.world.member(runtime, "D4initiR_LAL16java.lang.String_EO");
        self.call_global(init, &[module, argc as u64, argv as u64])
    }

    fn shutdown_runtime(&mut self) -> Flow<()> {
        let runtime = self.world.top(RUNTIME);
        let module = self.module(runtime)? as u64;
        let shutdown = self.world.member(runtime, "D10onShutdownuEO");
        self.call_global(shutdown, &[module]).map(drop)
    }
}
