//! Enum cases as values of the enum class, with ordinals (dotty's `DesugarEnums`).

use super::Desugar;
use super::class_syms::{CaseKind, EnumCount};
use crate::ast::constant::Constant;
use crate::ast::flags::{self, FlagSet};
use crate::ast::modifiers::Modifiers;
use crate::ast::span::Span;
use crate::ast::{Attrs, NodeId, TName, Tree};
use crate::names::to_utf16;

impl Desugar<'_> {
    /// The ordinal of the next case `name` of kind `kind`, and the lookup methods of the enum if
    /// the case is the last one (dotty's `nextOrdinal`).
    pub fn next_ordinal(&mut self, name: TName, kind: CaseKind, defines_lookups: bool) -> (u32, Vec<NodeId>) {
        let seen = self.scope.class.and_then(|c| self.classes[c].enum_count.take()).unwrap_or(EnumCount {
            ordinal: 0,
            min: CaseKind::Class,
            max: CaseKind::Simple,
            cases: Vec::new(),
        });
        let mut count = EnumCount {
            ordinal: seen.ordinal,
            min: kind.min(seen.min),
            max: kind.max(seen.max),
            cases: seen.cases,
        };
        if !name.is_type {
            count.cases.insert(0, (count.ordinal, name));
        }
        if defines_lookups {
            let this = self.ast.node(Tree::This { qual: None });
            let values: Vec<(u32, NodeId)> =
                count.cases.iter().rev().map(|&(i, name)| (i, self.select(this, name))).collect();
            let lookups = self.enum_lookup_methods(count.min, count.max, &values);
            return (count.ordinal, lookups);
        }
        let ordinal = count.ordinal;
        if let Some(c) = self.scope.class {
            self.classes[c].enum_count = Some(EnumCount { ordinal: ordinal + 1, ..count });
        }
        (ordinal, Vec::new())
    }

    /// The value of simple enum case `name` (dotty's `expandSimpleEnumCase`).
    pub fn expand_simple_enum_case(
        &mut self,
        name: TName,
        mods: Modifiers,
        lookups: bool,
        span: Span,
    ) -> NodeId {
        let Some(enum_class) = self.enum_class() else { return self.ast.node(Tree::Empty) };
        if !self.classes[enum_class].tparams.is_empty() {
            let parent = self.interpolated_enum_parent(enum_class, span);
            let constr = self.empty_constructor();
            let parents = self.list(&[parent]);
            let body = self.list(&[]);
            let imp =
                Tree::Template { constr, parents_or_derived: parents, self_: None, body, derived: None };
            let imp = self.ast.node(imp);
            return self.expand_enum_module(name, imp, mods, lookups, span);
        }
        let (ordinal, scaffolding) = self.next_ordinal(name, CaseKind::Simple, lookups);
        let new = self.term("$new");
        let new = self.ident(new);
        let ordinal = self.literal(Constant::Int(ordinal as i32));
        let text = self.literal(Constant::String(to_utf16(self.text(name)).into()));
        let creator = self.apply(new, &[ordinal, text]);
        let tpt = self.enum_class_ref();
        let mods = self.with_added_flags(mods, flags::ENUM_VALUE, span);
        let mods = self.ast.mods(mods);
        let vdef = self.ast.node(Tree::ValDef { name, tpt, rhs: Some(creator), mods });
        let expanded = self.flat_tree(&[&[vdef][..], &scaffolding].concat());
        self.ast.respan(expanded, span)
    }

    /// The value of an enum case object with template `imp` (dotty's `expandEnumModule`).
    pub fn expand_enum_module(
        &mut self,
        name: TName,
        imp: NodeId,
        mods: Modifiers,
        lookups: bool,
        span: Span,
    ) -> NodeId {
        if self.enum_class().is_none() {
            return self.ast.node(Tree::Empty);
        }
        let Tree::Template { constr, parents_or_derived, self_, derived, .. } = *self.ast.tree(imp) else {
            return imp;
        };
        let items = self.ast.items(parents_or_derived).to_vec();
        let n_parents = items.len() - derived.unwrap_or(0) as usize;
        if n_parents == 0 {
            return self.expand_simple_enum_case(name, mods, lookups, span);
        }
        let (_, scaffolding) = self.next_ordinal(name, CaseKind::Object, lookups);
        let (runtime, enum_value) = (self.term("runtime"), self.type_name("EnumValue"));
        let runtime = self.scala_dot(runtime);
        let enum_value = self.select(runtime, enum_value);
        let parents = [&items[..n_parents], &[enum_value], &items[n_parents..]].concat();
        let parents_or_derived = self.list(&parents);
        let body = self.list(&[]);
        let imp1 = self.ast.derived(imp, Tree::Template { constr, parents_or_derived, self_, body, derived });
        self.ast.add_attrs(imp1, Attrs::SINGLETON_MIRROR);
        let new = self.ast.node(Tree::New { tpt: imp1 });
        let tpt = self.type_tree();
        let mods = self.with_added_flags(mods, flags::ENUM_VALUE, span);
        let mods = self.ast.mods(mods);
        let vdef = self.ast.node(Tree::ValDef { name, tpt, rhs: Some(new), mods });
        let expanded = self.flat_tree(&[&[vdef][..], &scaffolding].concat());
        self.ast.respan(expanded, span)
    }

    /// The enum class applied to the bounds of its variant type parameters, the lower bound of
    /// a covariant one and the upper of a contravariant one (dotty's `interpolatedEnumParent`
    /// with the oracle's empty bounds).
    fn interpolated_enum_parent(&mut self, enum_class: usize, span: Span) -> NodeId {
        let class = &self.classes[enum_class];
        let simple = class.full_name.rsplit('.').next().unwrap_or(&class.full_name).to_string();
        let (full, tparams) = (class.full_name.clone(), class.tparams.clone());
        let mut args = Vec::new();
        for (tparam, variance) in tparams {
            args.push(if variance.is(flags::COVARIANT) {
                "T:scala.Nothing".to_string()
            } else if variance.is(flags::CONTRAVARIANT) {
                "T:scala.Any".to_string()
            } else {
                let msg = format!(
                    "cannot determine type argument for enum parent class {simple},\ntype parameter type {} is invariant",
                    self.text(tparam)
                );
                self.diags.error_at(span.start(), span.end(), span.point(), msg.as_str());
                format!("<error {msg}>")
            });
        }
        let parent = self.sym_ref(&format!("{full}[{}]", args.join(" ")), true);
        self.ast.respan(parent, span)
    }

    /// A reference to the enum class of the current scope (dotty's `enumClassRef`).
    pub fn enum_class_ref(&mut self) -> NodeId {
        match self.enum_class() {
            Some(c) => {
                let full = self.classes[c].full_name.clone();
                self.sym_ref(&full, true)
            }
            None => self.type_tree(),
        }
    }

    /// The enum class applied to wildcards (dotty's `rawRef`).
    pub fn raw_enum_ref(&mut self) -> NodeId {
        let tycon = self.enum_class_ref();
        let arity = self.enum_class().map_or(0, |c| self.classes[c].tparams.len());
        if arity == 0 {
            return tycon;
        }
        let args: Vec<NodeId> = (0..arity)
            .map(|_| self.ast.node(Tree::TypeBoundsTree { lo: None, hi: None, alias: None }))
            .collect();
        let args = self.list(&args);
        self.ast.node(Tree::AppliedTypeTree { tpt: tycon, args })
    }

    /// `m` with `flags` added if they apply to the same kind of definition, else an error and
    /// just `flags` (dotty's `withAddedFlags`).
    pub fn with_added_flags(&mut self, m: Modifiers, flags: FlagSet, span: Span) -> Modifiers {
        if m.flags.is_all_of(flags) {
            return m;
        }
        let compatible = m.flags.is_empty()
            || flags.is_empty()
            || m.flags.is_term_flags() && flags.is_term_flags()
            || m.flags.is_type_flags() && flags.is_type_flags();
        if compatible {
            return m.or(flags);
        }
        let what = if flags.is_term_flags() { "values" } else { "types" };
        let msg = format!("{} is not allowed for {what}", flags.and_not(m.flags).flags_string());
        self.diags.error_at(span.start(), span.end(), span.point(), msg);
        Modifiers::with_flags(flags)
    }
}
