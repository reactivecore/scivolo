//! Macro expansion in the VM: the typer's macro calls run as the macro implementations' NIR,
//! their quotes unpickled into the typer's trees (ADR 0029).

mod host;
mod reflect;
mod tasty_string;

pub use typer::inlining::Macros;

use host::Host;
use syntax::ast::constant::Constant;
use syntax::source::SourceFile;
use tasty::Erased;
use typer::inlining::{Annotations, Expanded, Lent, MacroArg, MacroCall, MacroError};
use vm::{Thrown, Vm, VmError};

/// The universe, trees and names the typer lends the expansion.
pub type Tables<'l> = (&'l mut tasty::Universe, &'l mut typer::tpd::Tpd, &'l mut syntax::names::Names);

/// Runs macro implementations in `vm`, the positions of their calls in `sources`.
pub struct VmMacros<'v> {
    pub vm: &'v mut Vm,
    pub sources: Vec<SourceFile>,
}

impl Macros for VmMacros<'_> {
    fn expand(
        &mut self,
        lent: Lent<'_>,
        call: &MacroCall,
        annotations: &Annotations,
    ) -> Result<Expanded, MacroError> {
        let refused = |what: &str| MacroError::Unsupported(what.to_owned());
        let u = &mut *lent.0;
        let owner = u.symbol(call.method).owner.ok_or_else(|| refused("macro implementation"))?;
        let module = u.binary_name(owner);
        let sig = u.method_signature(call.method);
        let sig = sig.ok_or_else(|| refused("a macro implementation without a method signature"))?;
        let (params, _) = u.erased_method(call.method);
        if params.len() != call.args.len() {
            return Err(refused("a macro implementation with erased parameters"));
        }
        let literals = call.args.iter().zip(&params).map(|(a, p)| literal(a, p));
        let literals: Vec<_> = literals.collect::<Option<_>>().ok_or_else(|| {
            refused("a literal macro argument of another primitive type than its parameter")
        })?;
        let sources = std::mem::take(&mut self.sources);
        let mut host =
            Host::new((&mut *lent.0, &mut *lent.1, &mut *lent.2), call.owner, (call.expansion, sources));
        host.annotations = annotations.clone();
        let args: Vec<vm::MacroArg> = call
            .args
            .iter()
            .zip(literals)
            .map(|(a, lit)| match *a {
                MacroArg::Expr(t) => vm::MacroArg::Expr(host.add_tree(t)),
                MacroArg::Type(t) => vm::MacroArg::Type(host.add_type(t)),
                MacroArg::Quotes => vm::MacroArg::Quotes,
                MacroArg::Literal(_) => lit.expect("a literal macro argument"),
            })
            .collect();
        let (host, result) = self.vm.expand_macro(Box::new(host), (&module, &sig), &args);
        let mut host: Box<Host> = (host as Box<dyn std::any::Any>).downcast().expect("the macro's host");
        host.restore((lent.0, lent.1, lent.2));
        self.sources = std::mem::take(&mut host.sources);
        match (result, host.refusal.take()) {
            (_, Some(why)) => Err(MacroError::Unsupported(why)),
            _ if !host.reported.is_empty() => Err(MacroError::Reported(std::mem::take(&mut host.reported))),
            (Ok(result), None) => Ok(Expanded { exprs: host.quoted().map_err(refused)?, result }),
            (Err(VmError::Uncaught(thrown)), None) if thrown.classes.iter().any(|c| c == STOP) => {
                Err(MacroError::Aborted)
            }
            (Err(VmError::Uncaught(thrown)), None) => Err(failure(&thrown)),
            (Err(e), None) => {
                Err(MacroError::Unsupported(format!("a macro implementation the VM cannot run: {e}")))
            }
        }
    }
}

/// Argument `a` as the VM passes it to a parameter erased to `param` if it is a literal: unboxed
/// to a primitive of its type, else boxed by the descriptor letter of its type (dotty's
/// `interpretLiteral`); `None` for a primitive of another type.
fn literal(a: &MacroArg, param: &Erased) -> Option<Option<vm::MacroArg>> {
    let MacroArg::Literal(c) = a else { return Some(None) };
    let (tag, bits, text) = match c {
        Constant::Unit => (b'V', 0, None),
        &Constant::Boolean(b) => (b'Z', b as u64, None),
        &Constant::Byte(n) => (b'B', n as u8 as u64, None),
        &Constant::Short(n) => (b'S', n as u16 as u64, None),
        &Constant::Char(n) => (b'C', n as u64, None),
        &Constant::Int(n) => (b'I', n as u32 as u64, None),
        &Constant::Long(n) => (b'J', n as u64, None),
        &Constant::Float(x) => (b'F', x.to_bits() as u64, None),
        &Constant::Double(x) => (b'D', x.to_bits(), None),
        Constant::String(units) => (b'T', 0, Some(String::from_utf16_lossy(units))),
        Constant::Null => (b'N', 0, None),
    };
    Some(Some(match *param {
        Erased::Prim(p) if p as u8 == tag.to_ascii_lowercase() => vm::MacroArg::Unboxed(bits),
        Erased::Prim(_) => return None,
        _ => vm::MacroArg::Literal(tag, bits, text),
    }))
}

const STOP: &str = "scala.quoted.runtime.StopMacroExpansion";

/// The error dotty reports for exception `thrown`; a refusal for the VM's `InternalError` of what
/// it does not support.
fn failure(thrown: &Thrown) -> MacroError {
    let class = thrown.classes.first().map_or("", String::as_str);
    let message = thrown.header.split_once(": ").map_or("", |(_, m)| m);
    if let Some(what) = message.strip_prefix("scivolo: unsupported: ")
        && class == "java.lang.InternalError"
    {
        return MacroError::Unsupported(format!("a macro implementation the VM cannot run: {what}"));
    }
    let mut msg = format!("Failed to evaluate macro.\n  Caused by class {class}: {message}");
    for f in thrown.frames.iter().skip(1) {
        let at = f.file.as_ref().map_or_else(String::new, |file| format!("{file}:{}", f.line));
        msg.push_str(&format!("\n    {}.{}({at})", f.class, f.method));
    }
    MacroError::Failed(msg)
}
