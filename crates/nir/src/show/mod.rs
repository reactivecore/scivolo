//! Text form of definitions, following Scala Native's `nir.Show`.

mod inst;
mod op;
mod value;

use crate::demangle;
use crate::nir::{Attr, Defn, DefnKind, Global, GlobalId, Inst, Local, Next};
use crate::pool::Pool;
use rustc_hash::FxHashMap;
use std::fmt::Write;

pub struct Printer<'p> {
    pool: &'p Pool,
    readable: bool,
    names: FxHashMap<u64, &'p str>,
    out: String,
}

pub fn defn(pool: &Pool, d: &Defn, readable: bool) -> String {
    let mut p = Printer { pool, readable, names: FxHashMap::default(), out: String::new() };
    p.defn(d);
    p.out
}

pub fn op(pool: &Pool, op: &crate::nir::Op) -> String {
    let mut p = Printer { pool, readable: true, names: FxHashMap::default(), out: String::new() };
    p.op(op);
    p.out
}

pub fn global(pool: &Pool, g: GlobalId, readable: bool) -> String {
    let mut p = Printer { pool, readable, names: FxHashMap::default(), out: String::new() };
    p.global(g);
    p.out
}

impl<'p> Printer<'p> {
    fn s(&mut self, s: &str) {
        self.out.push_str(s);
    }

    fn sep<T>(&mut self, items: &[T], sep: &str, mut f: impl FnMut(&mut Self, &T)) {
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                self.s(sep);
            }
            f(self, item);
        }
    }

    fn global(&mut self, g: GlobalId) {
        let pool = self.pool;
        if !self.readable {
            let _ = write!(self.out, "@\"{}\"", pool.mangle(g).replace('"', "\\\""));
            return;
        }
        match pool.global(g) {
            Global::None => self.s("@<none>"),
            Global::Top(s) => {
                let _ = write!(self.out, "@{}", pool.str(*s));
            }
            Global::Member(owner, sig) => {
                let _ = write!(self.out, "@{}::{}", pool.top_name(*owner), demangle::sig(pool.str(*sig)));
            }
        }
    }

    fn sig(&mut self, sig: &str) {
        match self.readable {
            true => self.s(&demangle::sig(sig)),
            false => {
                let _ = write!(self.out, "\"{sig}\"");
            }
        }
    }

    fn local(&mut self, l: Local) {
        let _ = write!(self.out, "%{}", l.0);
        if let Some(name) = self.names.get(&l.0) {
            let _ = write!(self.out, " <{name}>");
        }
    }

    fn attrs(&mut self, attrs: &[Attr]) {
        let pool = self.pool;
        for a in attrs {
            let _ = match a {
                Attr::BailOpt(m) => write!(self.out, "bailopt(\"{}\")", pool.str(*m)),
                Attr::Extern { blocking } => {
                    write!(self.out, "extern{}", if *blocking { " blocking" } else { "" })
                }
                Attr::Link(n) => write!(self.out, "link(\"{}\")", pool.str(*n)),
                Attr::Define(n) => write!(self.out, "define(\"{}\")", pool.str(*n)),
                Attr::Align { size, group } => match group {
                    Some(g) => write!(self.out, "align({size}, {})", pool.str(*g)),
                    None => write!(self.out, "align({size})"),
                },
                other => write!(self.out, "{}", attr_name(other)),
            };
            self.s(" ");
        }
    }

    fn parents(&mut self, parent: &Option<GlobalId>, traits: &[GlobalId]) {
        let all: Vec<GlobalId> = parent.iter().chain(traits).copied().collect();
        if !all.is_empty() {
            self.s(" : ");
            self.sep(&all, ", ", |p, g| p.global(*g));
        }
    }

    pub fn defn(&mut self, d: &Defn) {
        self.attrs(&d.attrs);
        match &d.kind {
            DefnKind::Var { ty, rhs } | DefnKind::Const { ty, rhs } => {
                self.s(if matches!(d.kind, DefnKind::Var { .. }) { "var " } else { "const " });
                self.global(d.name);
                self.s(" : ");
                self.ty(*ty);
                self.s(" = ");
                self.val(*rhs);
            }
            DefnKind::Declare { ty } => {
                self.s("decl ");
                self.global(d.name);
                self.s(" : ");
                self.ty(*ty);
            }
            DefnKind::Define { ty, body } => {
                let pool = self.pool;
                self.names = body.local_names.iter().map(|(l, s)| (l.0, pool.str(*s))).collect();
                self.s("def ");
                self.global(d.name);
                self.s(" : ");
                self.ty(*ty);
                self.s(" {");
                for inst in &body.insts {
                    self.s(if matches!(inst, Inst::Label(..)) { "\n" } else { "\n  " });
                    self.inst(inst);
                }
                self.s("\n}");
            }
            DefnKind::Trait { traits } => {
                self.s("trait ");
                self.global(d.name);
                self.parents(&None, traits);
            }
            DefnKind::Class { parent, traits } | DefnKind::Module { parent, traits } => {
                self.s(if matches!(d.kind, DefnKind::Class { .. }) { "class " } else { "module " });
                self.global(d.name);
                self.parents(parent, traits);
            }
        }
    }

    fn next(&mut self, n: &Next) {
        match n {
            Next::None => {}
            Next::Unwind(exc, to) => {
                self.s("unwind ");
                self.param(exc);
                self.s(" to ");
                self.next(to);
            }
            Next::Case(v, to) => {
                self.s("case ");
                self.val(*v);
                self.s(" => ");
                self.next(to);
            }
            Next::Label(l, args) => {
                self.local(*l);
                if !args.is_empty() {
                    self.s("(");
                    self.sep(args, ", ", |p, v| p.val(*v));
                    self.s(")");
                }
            }
        }
    }
}

fn attr_name(a: &Attr) -> &'static str {
    match a {
        Attr::MayInline => "mayinline",
        Attr::InlineHint => "inlinehint",
        Attr::NoInline => "noinline",
        Attr::AlwaysInline => "alwaysinline",
        Attr::MaySpecialize => "mayspecialize",
        Attr::NoSpecialize => "nospecialize",
        Attr::UnOpt => "unopt",
        Attr::NoOpt => "noopt",
        Attr::DidOpt => "didopt",
        Attr::Dyn => "dyn",
        Attr::Stub => "stub",
        Attr::LinkCppRuntime => "linkCppRuntime",
        Attr::Abstract => "abstract",
        Attr::Volatile => "volatile",
        Attr::Final => "final",
        Attr::SafePublish => "safe-publish",
        Attr::LinktimeResolved => "linktime",
        Attr::UsesIntrinsic => "uses-intrinsics",
        _ => "",
    }
}
