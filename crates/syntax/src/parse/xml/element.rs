//! The trees of elements with their namespace bindings and attributes (dotty's
//! `SymbolicXMLBuilder.element`).

use crate::ast::constant::Constant;
use crate::ast::flags;
use crate::ast::modifiers::Modifiers;
use crate::ast::{NodeId, Span, Tree};
use crate::parse::Parser;

impl Parser<'_> {
    /// Element `qname` with attributes `attrs` in the order written.
    pub fn xml_element(
        &mut self,
        span: Span,
        qname: &str,
        mut attrs: Vec<(String, NodeId)>,
        empty: bool,
        args: &[NodeId],
    ) -> NodeId {
        let mut namespaces = Vec::new();
        attrs.retain(|(key, value)| {
            if !key.starts_with("xmlns") {
                return true;
            }
            namespaces.push((key.clone(), *value));
            false
        });
        let namespaces: Vec<NodeId> =
            namespaces.into_iter().map(|(key, value)| self.namespace_binding(&key, value)).collect();
        let (pre, label) = match qname.split_once(':') {
            Some((p, x)) => (self.xml_const(p), x),
            None => (self.null_literal(), qname),
        };
        let synthetic = span.to_synthetic();
        let attributes: Vec<NodeId> =
            attrs.iter().rev().map(|(key, value)| self.attribute(synthetic, key, *value)).collect();
        let symbolic_attrs =
            if attributes.is_empty() { self.xml_term("Null") } else { self.xml_ident("$md") };
        let (attr_result, ns_result) = match (attributes.is_empty(), namespaces.is_empty()) {
            (true, true) => (Vec::new(), Vec::new()),
            (true, false) => (vec![self.scope_def()], [vec![self.tmp_scope_def()], namespaces].concat()),
            (false, true) => ([vec![self.metadata_def()], attributes].concat(), Vec::new()),
            (false, false) => (
                [vec![self.scope_def(), self.metadata_def()], attributes].concat(),
                [vec![self.tmp_scope_def()], namespaces].concat(),
            ),
        };
        let label = self.xml_const(label);
        let scope = self.xml_ident("$scope");
        let body = self.make_xml_elem(synthetic, [pre, label, symbolic_attrs, scope], empty, args);
        let inner = self.xml_block(&attr_result, body);
        let outer = self.xml_block(&ns_result, inner);
        self.at_span(synthetic, outer)
    }

    fn null_literal(&mut self) -> NodeId {
        self.ast.node(Tree::Literal { value: Constant::Null })
    }

    /// `$tmpscope = new NamespaceBinding(prefix, uri, $tmpscope)` for attribute `key`.
    fn namespace_binding(&mut self, key: &str, value: NodeId) -> NodeId {
        let prefix = match key.split_once(':') {
            Some((_, rest)) => self.xml_const(rest),
            None => self.null_literal(),
        };
        let uri = match *self.ast.tree(value) {
            Tree::Apply { args, .. } => match *self.ast.items(args) {
                [lit] if matches!(self.ast.tree(lit), Tree::Literal { .. }) => lit,
                _ => value,
            },
            _ => value,
        };
        let tmp = self.xml_ident("$tmpscope");
        let binding = self.xml_new("NamespaceBinding", &[prefix, uri, tmp]);
        let lhs = self.xml_ident("$tmpscope");
        self.ast.node(Tree::Assign { lhs, rhs: binding })
    }

    /// `$md = new (Un)PrefixedAttribute([pre,] key, value, $md)`.
    fn attribute(&mut self, span: Span, key: &str, value: NodeId) -> NodeId {
        let mut args = Vec::new();
        let (class, key) = match key.split_once(':') {
            Some((pre, rest)) => {
                args.push(self.xml_const(pre));
                ("PrefixedAttribute", rest)
            }
            None => ("UnprefixedAttribute", key),
        };
        args.push(self.xml_const(key));
        args.push(value);
        args.push(self.xml_ident("$md"));
        let rhs = self.xml_new(class, &args);
        let lhs = self.xml_ident("$md");
        let t = self.ast.node(Tree::Assign { lhs, rhs });
        self.at_span(span, t)
    }

    fn val_def(&mut self, name: &str, tpt: NodeId, rhs: NodeId, mutable: bool) -> NodeId {
        let name = self.term(name);
        let flags = if mutable { flags::MUTABLE } else { flags::EMPTY };
        let mods = self.ast.mods(Modifiers::with_flags(flags));
        self.ast.node(Tree::ValDef { name, tpt, rhs: Some(rhs), mods })
    }

    fn scope_def(&mut self) -> NodeId {
        let (tpt, rhs) = (self.xml_type("NamespaceBinding"), self.xml_ident("$tmpscope"));
        self.val_def("$scope", tpt, rhs, false)
    }

    fn tmp_scope_def(&mut self) -> NodeId {
        let (tpt, rhs) = (self.xml_type("NamespaceBinding"), self.xml_ident("$scope"));
        self.val_def("$tmpscope", tpt, rhs, true)
    }

    fn metadata_def(&mut self) -> NodeId {
        let (tpt, rhs) = (self.xml_type("MetaData"), self.xml_term("Null"));
        self.val_def("$md", tpt, rhs, true)
    }
}
