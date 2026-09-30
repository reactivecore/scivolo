//! The lookup methods of an enum's companion: `$values`, `values`, `valueOf`, `$new` and
//! `fromOrdinal` (dotty's `enumLookupMethods`).

use super::Desugar;
use super::class_syms::CaseKind;
use crate::ast::constant::Constant;
use crate::ast::flags;
use crate::ast::tree::MatchKind;
use crate::ast::{Attrs, NodeId, Tree};
use crate::names::to_utf16;

impl Desugar<'_> {
    /// The lookup methods of an enum whose cases range from kind `min` to `max`, with the value
    /// cases `values` and their ordinals.
    pub fn enum_lookup_methods(
        &mut self,
        min: CaseKind,
        max: CaseKind,
        values: &[(u32, NodeId)],
    ) -> Vec<NodeId> {
        if self.enum_class().is_none() {
            return Vec::new();
        }
        let is_enumeration = max < CaseKind::Class;
        let mut methods = Vec::new();
        if is_enumeration {
            let refs: Vec<NodeId> = values.iter().map(|&(_, v)| v).collect();
            methods.extend(self.enum_scaffolding(&refs));
        }
        if min == CaseKind::Simple {
            methods.push(self.enum_value_creator());
        }
        methods.push(self.lookup_by_ordinal(min < CaseKind::Class, is_enumeration, values));
        methods
    }

    /// `$values`, `values` and `valueOf` over the value cases `values`.
    fn enum_scaffolding(&mut self, values: &[NodeId]) -> Vec<NodeId> {
        let raw = self.raw_enum_ref();
        let array = self.array_literal(values, raw);
        let values_name = self.term("$values");
        let tpt = self.type_tree();
        let mods = self.mods(flags::PRIVATE.or(flags::SYNTHETIC));
        let private_values = self.ast.node(Tree::ValDef { name: values_name, tpt, rhs: Some(array), mods });
        let array_type = self.sym_ref("scala.Array", true);
        let args = self.list(&[raw]);
        let values_type = self.ast.node(Tree::AppliedTypeTree { tpt: array_type, args });
        let clone = self.values_dot("clone");
        let mods = self.mods(flags::SYNTHETIC);
        let name = self.term("values");
        let values_def = self.method(name, &[], Some(values_type), clone, mods);
        let name_param = self.term("$name");
        let default =
            self.lookup_error("name", |d| d.ident(name_param), "java.lang.IllegalArgumentException");
        let mut cases: Vec<NodeId> = values
            .iter()
            .map(|&v| {
                let Tree::Select { name, .. } = *self.ast.tree(v) else { unreachable!("a case value") };
                let lit = self.literal(Constant::String(to_utf16(self.text(name)).into()));
                self.ast.node(Tree::CaseDef { pat: lit, guard: None, body: v })
            })
            .collect();
        cases.push(default);
        let selector = self.ident(name_param);
        let cases = self.list(&cases);
        let body = self.ast.node(Tree::Match { selector: Some(selector), cases, kind: MatchKind::Plain });
        let param = self.enum_param("$name", "java.lang.String");
        let clause = self.list(&[param]);
        let mods = self.mods(flags::SYNTHETIC);
        let name = self.term("valueOf");
        let value_of = self.method(name, &[clause], None, body, mods);
        vec![private_values, values_def, value_of]
    }

    /// `Array[tpt](values)(using ClassTag[tpt](classOf[tpt]))`.
    fn array_literal(&mut self, values: &[NodeId], tpt: NodeId) -> NodeId {
        let class_of = self.sym_ref("scala.Predef$.classOf", false);
        let args = self.list(&[tpt]);
        let class_of = self.ast.node(Tree::TypeApply { fun: class_of, args });
        let class_tag = self.sym_ref("scala.reflect.ClassTag$.apply", false);
        let args = self.list(&[tpt]);
        let class_tag = self.ast.node(Tree::TypeApply { fun: class_tag, args });
        let class_tag = self.apply(class_tag, &[class_of]);
        let array = self.sym_ref("scala.Array", false);
        let apply = self.term("apply");
        let apply = self.select(array, apply);
        let args = self.list(&[tpt]);
        let apply = self.ast.node(Tree::TypeApply { fun: apply, args });
        let apply = self.apply(apply, values);
        let apply = self.apply(apply, &[class_tag]);
        self.ast.add_attrs(apply, Attrs::USING_APPLY);
        apply
    }

    /// `private def $new(_$ordinal: Int, $name: String) = new E with scala.runtime.EnumValue`.
    fn enum_value_creator(&mut self) -> NodeId {
        let constr = self.empty_constructor();
        let class = self.enum_class_ref();
        let (runtime, enum_value) = (self.term("runtime"), self.type_name("EnumValue"));
        let runtime = self.scala_dot(runtime);
        let enum_value = self.select(runtime, enum_value);
        let parents = self.list(&[class, enum_value]);
        let body = self.list(&[]);
        let imp = Tree::Template { constr, parents_or_derived: parents, self_: None, body, derived: None };
        let imp = self.ast.node(imp);
        self.ast.add_attrs(imp, Attrs::SINGLETON_MIRROR);
        let creator = self.ast.node(Tree::New { tpt: imp });
        let ordinal = self.enum_param("_$ordinal", "scala.Int");
        let name = self.enum_param("$name", "java.lang.String");
        let clause = self.list(&[ordinal, name]);
        let mods = self.mods(flags::PRIVATE.or(flags::SYNTHETIC));
        let name = self.term("$new");
        self.method(name, &[clause], None, creator, mods)
    }

    /// `def fromOrdinal(ordinal: Int): E`, looking the value up in `$values` or among `values`,
    /// or throwing without cached values.
    fn lookup_by_ordinal(&mut self, cached: bool, is_enumeration: bool, values: &[(u32, NodeId)]) -> NodeId {
        let ordinal_name = self.term("ordinal");
        let param = self.enum_param("ordinal", "scala.Int");
        let raw = self.raw_enum_ref();
        let ordinal = self.ident(ordinal_name);
        let to_string = self.term("toString");
        let body = if !cached {
            let (_, throw) = self.lookup_throw(
                "ordinal",
                |d| d.select(ordinal, to_string),
                "java.util.NoSuchElementException",
            );
            throw
        } else {
            let default = self.lookup_error(
                "ordinal",
                |d| d.select(ordinal, to_string),
                "java.util.NoSuchElementException",
            );
            if is_enumeration {
                let apply = self.values_dot("apply");
                let expr = self.apply(apply, &[ordinal]);
                let cases = self.list(&[default]);
                self.ast.node(Tree::Try { expr, cases, finalizer: None })
            } else {
                let mut cases: Vec<NodeId> = values
                    .iter()
                    .map(|&(i, v)| {
                        let lit = self.literal(Constant::Int(i as i32));
                        self.ast.node(Tree::CaseDef { pat: lit, guard: None, body: v })
                    })
                    .collect();
                cases.push(default);
                let cases = self.list(&cases);
                self.ast.node(Tree::Match { selector: Some(ordinal), cases, kind: MatchKind::Plain })
            }
        };
        let clause = self.list(&[param]);
        let mods = self.mods(flags::SYNTHETIC);
        let name = self.term("fromOrdinal");
        self.method(name, &[clause], Some(raw), body, mods)
    }

    /// `case _ => throw new X("enum E has no case with <what>: " + arg)`.
    fn lookup_error(&mut self, what: &str, arg: impl FnOnce(&mut Self) -> NodeId, exception: &str) -> NodeId {
        let (wildcard, throw) = self.lookup_throw(what, arg, exception);
        self.ast.node(Tree::CaseDef { pat: wildcard, guard: None, body: throw })
    }

    fn lookup_throw(
        &mut self,
        what: &str,
        arg: impl FnOnce(&mut Self) -> NodeId,
        exception: &str,
    ) -> (NodeId, NodeId) {
        let full = self.enum_class().map_or(String::new(), |c| self.classes[c].full_name.clone());
        let text = format!("enum {full} has no case with {what}: ");
        let lit = self.literal(Constant::String(to_utf16(&text).into()));
        let plus = self.term("+");
        let plus = self.select(lit, plus);
        let arg = arg(self);
        let msg = self.apply(plus, &[arg]);
        let tpt = self.sym_ref(exception, true);
        let nu = self.make_new(tpt);
        let nu = self.apply(nu, &[msg]);
        let throw = self.ast.node(Tree::Throw { expr: nu });
        let wildcard = self.term("_");
        (self.ident(wildcard), throw)
    }

    /// `$values.name`.
    fn values_dot(&mut self, name: &str) -> NodeId {
        let values = self.term("$values");
        let values = self.ident(values);
        let name = self.term(name);
        self.select(values, name)
    }

    /// Parameter `name: tpe` of a lookup method.
    fn enum_param(&mut self, name: &str, tpe: &str) -> NodeId {
        let name = self.term(name);
        let tpt = self.sym_ref(tpe, true);
        let mods = self.mods(flags::PARAM);
        self.ast.node(Tree::ValDef { name, tpt, rhs: None, mods })
    }
}
