// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inliner.scala
//! Proxies of inline arguments, and the trees standing for bound parameters in an expansion
//! (dotty's `paramBindingDef`, and the parameter case of the inliner's tree map).

use super::copy::TreeTypeMap;
use super::expand::Expansion;
use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashMap;
use syntax::ast::{Span, TName};
use tasty::tags::{INLINE, INLINEPROXY};
use tasty::{Flags, Kind, Symbol, SymbolId, Type, TypeId};

impl Expansion<'_, '_> {
    /// The type of a new proxy binding argument `arg` of parameter `p` of type `formal`: a value,
    /// or a method for a by-name parameter; typed by `skolem` if the call's type refers to the
    /// argument by one.
    pub fn proxy(
        &mut self,
        p: SymbolId,
        formal: TypeId,
        arg: TNodeId,
        skolem: Option<TypeId>,
    ) -> Result<TypeId, Unsupported> {
        let i = &mut *self.i;
        let mut arg_tpe = i.tpd.tpe(arg);
        if let T::Typed { expr, .. } = *i.tpd.tree(arg)
            && let Some(seq) = i.seq_of_repeated(arg_tpe)
        {
            let array = i.scala_member("Array", Kind::Class);
            match i.tpd.get(expr).tpe.map(|t| i.u.widen(t)) {
                Some(t) if i.u.class_symbol(t).is_some_and(|c| Some(c) == array) => {
                    return Err(self.err("array passed to a repeated parameter of an inline method"));
                }
                Some(_) => {}
                None if matches!(i.tpd.tree(expr), T::SeqLiteral { .. }) => {
                    i.tpd.get_mut(expr).tpe = Some(seq)
                }
                None => return Err(self.err("untyped repeated inline argument")),
            }
            arg_tpe = seq;
        }
        let by_name = matches!(i.u.types().get(formal), Type::ByName(_));
        let inline = i.is_inline_param_type(formal);
        let mut widened = i.u.widen(arg_tpe);
        let builtins = i.u.builtins();
        let bottom = i.u.class_symbol(widened).is_some_and(|c| c == builtins.nothing || c == builtins.null);
        let skolem = skolem.filter(|_| !bottom);
        if let Some(sk) = skolem {
            widened = sk;
        }
        let info = if bottom {
            widened = match *i.u.types().get(formal) {
                Type::ByName(r) => r,
                _ => formal,
            };
            formal
        } else if by_name {
            i.u.intern_type(Type::ByName(widened))
        } else {
            widened
        };
        let mut flags = Flags::default().with(INLINEPROXY);
        if inline {
            flags = flags.with(INLINE);
        }
        let base = i.u.symbol(p).name.clone();
        let name = i.fresh.next(i.names, &format!("{base}$proxy"));
        let text = i.names.text(name).to_owned();
        let kind = if by_name { Kind::Def } else { Kind::Val };
        let s = i.u.define(Symbol {
            name: text,
            kind,
            owner: Some(self.owner),
            flags,
            target: None,
            is_extern: false,
            derived: true,
            decls: Vec::new(),
            tree: None,
        });
        i.u.set_info(s, info);
        let mut rhs = match i.defines_class(arg) {
            true => i.change_owner(arg, self.owner, s),
            false => {
                let mut m =
                    TreeTypeMap { old_owner: Some(self.owner), new_owner: Some(s), ..TreeTypeMap::default() };
                i.copy(&mut m, arg, &mut |_, _, _| None)?
            }
        };
        if let Some(sk) = skolem {
            rhs = i.cast(rhs, sk)?;
        }
        if bottom && inline {
            let at = i.tpd.span(rhs);
            let tpt = i.tpd.node(T::TypeTree { inferred: false }, at.end_pos(), Some(widened), None);
            rhs = i.tpd.node(T::Typed { expr: rhs, tpt }, at.to_synthetic(), Some(widened), None);
        }
        let span = i.tpd.span(self.call);
        let tpt_span = Span::at(i.tpd.span(arg).start());
        let name = TName { name, is_type: false };
        let def = if by_name {
            let tpt = i.tpd.node(T::TypeTree { inferred: false }, tpt_span, Some(widened), None);
            let paramss = i.tpd.clauses(&[]);
            T::DefDef { name, paramss, tpt, rhs: Some(rhs) }
        } else {
            let tpt = i.tpd.node(T::TypeTree { inferred: true }, tpt_span, Some(info), None);
            T::ValDef { name, tpt, rhs: Some(rhs) }
        };
        self.bindings.push(i.tpd.node(def, span, None, Some(s)));
        Ok(i.u.intern_type(Type::Sym(None, s)))
    }
}

/// The tree replacing `t` in the copied body, if `t` refers to a bound parameter: the binding's
/// singleton as an argument inlined from outside, or the type argument.
pub fn replace_param(
    i: &mut Inlining<'_>,
    binding: &FxHashMap<SymbolId, TypeId>,
    arg_span: &FxHashMap<SymbolId, Span>,
    t: TNodeId,
) -> Option<TNodeId> {
    let T::Ident { name } = *i.tpd.tree(t) else { return None };
    let s = i.tpd.sym(t)?;
    let &bound = binding.get(&s)?;
    if i.names.text(name.name) == "_" {
        let sym = match *i.u.types().get(bound) {
            Type::Sym(_, c) => Some(c),
            _ => None,
        };
        return Some(i.tpd.node(T::Ident { name }, i.tpd.span(t), Some(bound), sym));
    }
    let span = *arg_span.get(&s)?;
    if name.is_type {
        return Some(i.tpd.node(T::TypeTree { inferred: true }, span, Some(bound), None));
    }
    let inner = i.singleton(bound, span)?;
    let empty = i.tpd.list(&[]);
    let tpe = i.tpd.tpe(inner);
    Some(i.tpd.node(
        T::Inlined { call: None, bindings: empty, expansion: inner },
        i.tpd.span(t),
        Some(tpe),
        None,
    ))
}

impl Inlining<'_> {
    /// Whether `t` defines symbols.
    pub fn has_definitions(&self, t: TNodeId) -> bool {
        matches!(
            self.tpd.tree(t),
            T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. } | T::Bind { .. } | T::Closure { .. }
        ) || self.tpd.children(t).into_iter().any(|c| self.has_definitions(c))
    }

    /// Whether `t` defines a class (a synthesized anonymous mirror).
    pub fn defines_class(&self, t: TNodeId) -> bool {
        let class = |i: &Self, s: Option<SymbolId>| s.is_some_and(|s| i.u.symbol(s).kind == Kind::Class);
        matches!(self.tpd.tree(t), T::TypeDef { .. }) && class(self, self.tpd.sym(t))
            || self.tpd.children(t).into_iter().any(|c| self.defines_class(c))
    }

    /// `t` itself with the definitions owned by `old` given to `new` (dotty's `changeOwner`); the
    /// binding then shares `t` with the call the `Inlined` tree keeps, which the lowering drops.
    pub fn change_owner(&mut self, t: TNodeId, old: SymbolId, new: SymbolId) -> TNodeId {
        let mut stack = vec![t];
        while let Some(n) = stack.pop() {
            let defines = matches!(self.tpd.tree(n), T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. });
            if let Some(s) = self.tpd.sym(n).filter(|_| defines)
                && self.u.symbol(s).owner == Some(old)
            {
                self.u.set_owner(s, new);
            }
            stack.extend(self.tpd.children(n));
        }
        t
    }

    /// `Seq[T]` if `t` is `<repeated>[T]` (dotty's `translateFromRepeated(toArray = false)`).
    fn seq_of_repeated(&mut self, t: TypeId) -> Option<TypeId> {
        let Type::Applied(c, args) = self.u.types().get(t).clone() else { return None };
        if args.len() != 1 || self.u.class_symbol(c) != Some(self.scala_member("<repeated>", Kind::Class)?) {
            return None;
        }
        let seq = self.u.class_ref("scala.collection.immutable.Seq");
        Some(self.u.intern_type(Type::Applied(seq, args)))
    }

    /// Whether parameter type `p` is annotated `@InlineParam`, also inside a by-name type.
    pub fn is_inline_param_type(&mut self, p: TypeId) -> bool {
        match *self.u.types().get(p) {
            Type::ByName(r) => self.is_inline_param_type(r),
            Type::Annotated(_, annot) => self
                .u
                .class_symbol(annot)
                .is_some_and(|c| self.u.full_name(c) == "scala.annotation.internal.InlineParam"),
            _ => false,
        }
    }
}
