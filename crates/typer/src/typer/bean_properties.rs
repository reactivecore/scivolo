// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/BeanProperties.scala
//! The accessors `@BeanProperty` and `@BooleanBeanProperty` add to a class (dotty's
//! `BeanProperties`): `def getX(): T = x` (`isX` for the boolean one) and, for a `var`,
//! `def setX(x: T): Unit = this.x = x`, invisible to Scala code.

use crate::ctx::Ctx;
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree as T};
use crate::typer::Typer;
use syntax::ast::flags::{self as f, FlagSet};
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::tags::MUTABLE;
use tasty::{SymbolId, Type};

/// The annotation class that adds a getter `getX`.
const BEAN_PROPERTY: &str = "scala.beans.BeanProperty";

/// The annotation class that adds a getter `isX`.
const BOOLEAN_BEAN_PROPERTY: &str = "scala.beans.BooleanBeanProperty";

/// The flags of a bean accessor.
const ACCESSOR: FlagSet = f::SYNTHETIC.or(f::INVISIBLE);

impl Typer<'_> {
    /// Types the bean accessors of the values among the typed statements `body` of a class in
    /// its context `cctx`, each getter before its setter, with the annotations of the value
    /// meant for bean getters or setters (`@beanGetter`, `@beanSetter`).
    pub fn bean_accessors(&mut self, body: &[TNodeId], cctx: Ctx) -> Vec<TNodeId> {
        let mut defs: Vec<(NodeId, SymbolId, &str)> = Vec::new();
        for &stat in body {
            let (T::ValDef { .. }, Some(value)) = (self.tpd.tree(stat), self.tpd.sym(stat)) else { continue };
            let Some((prefix, span)) = self.bean_annotation(value) else { continue };
            let name = self.u.symbol(value).name.clone();
            let capitalized = capitalize(&name);
            let getter = self.bean_getter(value, &format!("{prefix}{capitalized}"), span);
            defs.push((getter, value, "beanGetter"));
            if self.u.symbol(value).flags.has(MUTABLE) {
                let setter = self.bean_setter(value, &format!("set{capitalized}"), span);
                defs.push((setter, value, "beanSetter"));
            }
        }
        if defs.is_empty() {
            return Vec::new();
        }

        let trees: Vec<NodeId> = defs.iter().map(|&(d, _, _)| d).collect();
        self.index(&trees, cctx);
        for &(d, value, target) in &defs {
            let accessor = self.syms.by_tree[&d];
            let carried = self.u.annotations_carrying(value, &[target], &[]);
            self.copy_annotations_where(value, accessor, |_, class| carried.contains(&class));
        }
        trees.into_iter().map(|d| self.typed_stat(d, cctx)).collect()
    }

    /// The getter prefix of the bean annotation of `value` and the annotation's span, if it has
    /// one.
    fn bean_annotation(&mut self, value: SymbolId) -> Option<(&'static str, Span)> {
        let classes = self.u.annotations(value).to_vec();
        if !classes
            .into_iter()
            .any(|c| [BEAN_PROPERTY, BOOLEAN_BEAN_PROPERTY].contains(&&*self.u.full_name(c)))
        {
            return None;
        }
        let queued: Vec<_> = self.syms.annotation_trees.iter().filter(|a| a.0 == value).copied().collect();
        for (_, annot, ctx) in queued {
            let Some((class, _)) = self.annotation_class(annot, ctx) else { continue };
            let prefix = match self.u.full_name(class).as_str() {
                BEAN_PROPERTY => "get",
                BOOLEAN_BEAN_PROPERTY => "is",
                _ => continue,
            };
            return Some((prefix, self.ast.span(annot).start_pos()));
        }
        None
    }

    /// An untyped tree standing for `C.this.value`, `value` a member of class `C`, an identifier
    /// if it is a parameter accessor (dotty's `ref`).
    fn splice_member(&mut self, value: SymbolId, span: Span) -> NodeId {
        if self.syms.flags_of(value).is(f::PARAM_ACCESSOR) {
            return self.splice_ref(value);
        }
        let owner = self.u.symbol(value).owner.expect("a member");
        let class = self.u.intern_type(Type::Sym(None, owner));
        let this = self.build().this(class, span);
        let select = self.build().select(this, value, span);
        let id = self.ast.node(Tree::TypeTree);
        self.splices.insert(id, select);
        id
    }

    /// `def name(): T = value` at `span`.
    fn bean_getter(&mut self, value: SymbolId, name: &str, span: Span) -> NodeId {
        let info = self.u.info(value);
        let tpt = self.splice_type(info);
        let rhs = self.splice_member(value, span);
        let def = self.def_def(name, &[], tpt, Some(rhs), ACCESSOR);
        self.ast.with_span(def, span);
        def
    }

    /// `def name(x: T): Unit = this.x = x` at `span`, the parameter named as `value`.
    fn bean_setter(&mut self, value: SymbolId, name: &str, span: Span) -> NodeId {
        let info = self.u.info(value);
        let param_name = self.u.symbol(value).name.clone();
        let param_name = TName { name: self.name(&param_name), is_type: false };
        let tpt = self.splice_type(info);
        let mods = self.ast.mods(Modifiers::with_flags(f::TERM_PARAM));
        let param = self.ast.node(Tree::ValDef { name: param_name, tpt, rhs: None, mods });
        let lhs = self.splice_member(value, span);
        let rhs = self.ast.node(Tree::Ident { name: param_name });
        let assign = self.ast.node(Tree::Assign { lhs, rhs });
        let unit = type_ref(self.u, self.defn.unit);
        let unit = self.splice_type(unit);
        let def = self.def_def(name, &[param], unit, Some(assign), ACCESSOR);
        self.ast.with_span(def, span);
        def
    }
}

/// `name` with its first letter in upper case (Scala's `capitalize`).
fn capitalize(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}
