//! Macro expansion in the VM: the typer's macro calls run as the macro implementations' NIR,
//! their quotes unpickled into the typer's trees (ADR 0029).

mod host;
mod tasty_string;

pub use typer::inlining::Macros;

use host::Host;
use typer::inlining::{Expanded, Lent, MacroArg, MacroCall, MacroError};
use vm::{Thrown, Vm, VmError};

/// What the `Unsupported` exceptions of the overlay's `QuotesImpl` name.
const UNSUPPORTED: [&str; 13] = [
    "a quote the frontend fails on",
    "quotes.reflect",
    "Expr.show",
    "Expr.matches",
    "Expr.valueOrAbort",
    "Expr.isExprOf",
    "Expr.asExprOf",
    "a quote of Scala 3.0 or 3.1",
    "a quoted type of Scala 3.0 or 3.1",
    "a quote pattern",
    "a quoted type pattern",
    "an Expr made outside the frontend",
    "a Type made outside the frontend",
];

const UNSUPPORTED_CLASS: &str = "scala.quoted.runtime.impl.Unsupported";

/// Runs macro implementations in `vm`.
pub struct VmMacros<'v> {
    pub vm: &'v mut Vm,
}

impl Macros for VmMacros<'_> {
    fn expand(&mut self, lent: Lent<'_>, call: &MacroCall) -> Result<Expanded, MacroError> {
        let u = &mut *lent.0;
        let owner = u.symbol(call.method).owner.ok_or(MacroError::Unsupported("macro implementation"))?;
        let module = u.binary_name(owner);
        let sig = u.method_signature(call.method);
        let sig = sig.ok_or(MacroError::Unsupported("a macro implementation without a method signature"))?;
        let mut host = Host::new((&mut *lent.0, &mut *lent.1, &mut *lent.2, &mut *lent.3), call.owner);
        let args: Vec<vm::MacroArg> = call
            .args
            .iter()
            .map(|a| match *a {
                MacroArg::Expr(t) => vm::MacroArg::Expr(host.add_tree(t)),
                MacroArg::Type(t) => vm::MacroArg::Type(host.add_type(t)),
                MacroArg::Quotes => vm::MacroArg::Quotes,
            })
            .collect();
        let (host, result) = self.vm.expand_macro(Box::new(host), (&module, &sig), &args);
        let mut host: Box<Host> = (host as Box<dyn std::any::Any>).downcast().expect("the macro's host");
        host.restore((&mut *lent.0, &mut *lent.1, &mut *lent.2, &mut *lent.3));
        match (result, host.refusal) {
            (_, Some(why)) => Err(MacroError::Unsupported(why)),
            (Ok(result), None) => {
                Ok(Expanded { exprs: host.quoted().map_err(MacroError::Unsupported)?, result })
            }
            (Err(VmError::Uncaught(thrown)), None) => Err(failure(&thrown)),
            (Err(_), None) => Err(MacroError::Unsupported("a macro implementation the VM cannot run")),
        }
    }
}

/// The refusal of `Unsupported`, else the error dotty reports for exception `thrown`.
fn failure(thrown: &Thrown) -> MacroError {
    let class = thrown.classes.first().map_or("", String::as_str);
    let message = thrown.header.split_once(": ").map_or("", |(_, m)| m);
    if class == UNSUPPORTED_CLASS {
        let what = UNSUPPORTED.iter().find(|&&w| w == message);
        return MacroError::Unsupported(what.copied().unwrap_or("a Quotes operation outside the subset"));
    }
    let mut msg = format!("Failed to evaluate macro.\n  Caused by class {class}: {message}");
    for f in thrown.frames.iter().skip(1) {
        let at = f.file.as_ref().map_or_else(String::new, |file| format!("{file}:{}", f.line));
        msg.push_str(&format!("\n    {}.{}({at})", f.class, f.method));
    }
    MacroError::Failed(msg)
}
