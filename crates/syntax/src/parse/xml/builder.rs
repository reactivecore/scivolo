// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/xml/SymbolicXMLBuilder.scala
//! The `scala.xml` trees of XML literals (dotty's `SymbolicXMLBuilder`), but for elements.

use crate::ast::constant::Constant;
use crate::ast::{Attrs, NodeId, Span, TName, Tree};
use crate::names::to_utf16;
use crate::parse::Parser;

impl Parser<'_> {
    /// `_root_.scala.xml.<name>`.
    pub fn xml_path(&mut self, name: TName) -> NodeId {
        self.scala_path(&["xml"], name)
    }

    pub fn xml_type(&mut self, name: &str) -> NodeId {
        let name = self.type_name(name);
        self.xml_path(name)
    }

    pub fn xml_term(&mut self, name: &str) -> NodeId {
        let name = self.term(name);
        self.xml_path(name)
    }

    pub fn xml_ident(&mut self, name: &str) -> NodeId {
        let name = self.term(name);
        self.ast.node(Tree::Ident { name })
    }

    pub fn xml_const(&mut self, text: &str) -> NodeId {
        self.xml_const_units(to_utf16(text))
    }

    fn xml_const_units(&mut self, units: Vec<u16>) -> NodeId {
        self.ast.node(Tree::Literal { value: Constant::String(units.into_boxed_slice()) })
    }

    /// `new scala.xml.<class>(args)`.
    pub fn xml_new(&mut self, class: &str, args: &[NodeId]) -> NodeId {
        let tpt = self.xml_type(class);
        self.make_new(tpt, args)
    }

    /// A text node, or a `Text` pattern.
    pub fn xml_text(&mut self, span: Span, text: &str) -> NodeId {
        self.xml_text_units(span, to_utf16(text))
    }

    pub fn xml_text_units(&mut self, span: Span, units: Vec<u16>) -> NodeId {
        let lit = self.xml_const_units(units);
        let t = if self.xml.is_pattern { self.make_text_pat(lit) } else { self.xml_new("Text", &[lit]) };
        self.at_span(span, t)
    }

    /// `scala.xml.Text(t)` as a pattern.
    fn make_text_pat(&mut self, t: NodeId) -> NodeId {
        let fun = self.xml_term("Text");
        let args = self.ast.list(&[t]);
        self.ast.node(Tree::Apply { fun, args })
    }

    pub fn entity_ref(&mut self, span: Span, name: &str) -> NodeId {
        let c = self.xml_const(name);
        let t = self.xml_new("EntityRef", &[c]);
        self.at_span(span, t)
    }

    pub fn xml_comment(&mut self, span: Span, text: &str) -> NodeId {
        let c = self.xml_const(text);
        let t = self.xml_new("Comment", &[c]);
        self.at_span(span, t)
    }

    pub fn xml_char_data(&mut self, span: Span, text: &str) -> NodeId {
        let c = self.xml_const(text);
        let t = self.xml_new("Text", &[c]);
        self.at_span(span, t)
    }

    pub fn xml_proc_instr(&mut self, span: Span, target: &str, text: &str) -> NodeId {
        let (target, text) = (self.xml_const(target), self.xml_const(text));
        let t = self.xml_new("ProcInstr", &[target, text]);
        self.at_span(span, t)
    }

    pub fn xml_unparsed(&mut self, span: Span, text: &str) -> NodeId {
        let c = self.xml_const(text);
        let t = self.xml_new("Unparsed", &[c]);
        self.at_span(span, t)
    }

    pub fn xml_group(&mut self, span: Span, args: &[NodeId]) -> NodeId {
        let seq = self.make_xml_seq(span, args, true);
        let t = self.xml_new("Group", &[seq]);
        self.at_span(span, t)
    }

    /// `{ val $buf = new NodeBuffer(); $buf &+ arg; ...; $buf[.toVector] }`.
    pub fn make_xml_seq(&mut self, span: Span, args: &[NodeId], to_vector: bool) -> NodeId {
        let (buf_name, plus) = (self.term("$buf"), self.term("&+"));
        let tpt = self.type_tree();
        let rhs = self.xml_new("NodeBuffer", &[]);
        let mods = self.empty_mods();
        let mut stats = vec![self.ast.node(Tree::ValDef { name: buf_name, tpt, rhs: Some(rhs), mods })];
        for &t in args {
            if matches!(self.ast.tree(t), Tree::Literal { value: Constant::String(s) } if s.is_empty()) {
                continue;
            }
            let buf = self.ast.node(Tree::Ident { name: buf_name });
            let fun = self.ast.node(Tree::Select { qualifier: buf, name: plus });
            let args = self.ast.list(&[t]);
            stats.push(self.ast.node(Tree::Apply { fun, args }));
        }
        let buf = self.ast.node(Tree::Ident { name: buf_name });
        let res = if to_vector {
            let name = self.term("toVector");
            self.ast.node(Tree::Select { qualifier: buf, name })
        } else {
            buf
        };
        let b = self.xml_block(&stats, res);
        self.at_span(span, b)
    }

    /// dotty's `XMLBlock(stats, expr)`.
    pub fn xml_block(&mut self, stats: &[NodeId], expr: NodeId) -> NodeId {
        let stats = self.ast.list(stats);
        let b = self.ast.node(Tree::Block { stats, expr: Some(expr) });
        self.ast.add_attrs(b, Attrs::XML_BLOCK);
        b
    }

    /// The pattern `scala.xml.Elem(pre, label, _, _, children*)` for element `name`.
    pub fn make_xml_pat(&mut self, span: Span, name: &str, args: &[NodeId]) -> NodeId {
        let (pre, label) = match name.split_once(':') {
            Some((pre, rest)) => (self.xml_const(pre), self.xml_const(rest)),
            None => (self.xml_ident("_"), self.xml_const(name)),
        };
        let fun = self.xml_term("Elem");
        let mut all = vec![pre, label, self.xml_ident("_"), self.xml_ident("_")];
        for &t in args {
            let is_lit = matches!(self.ast.tree(t), Tree::Literal { .. });
            all.push(if is_lit { self.make_text_pat(t) } else { t });
        }
        let args = self.ast.list(&all);
        let t = self.ast.node(Tree::Apply { fun, args });
        self.at_span(span, t)
    }

    /// `new scala.xml.Elem(pre, label, attrs, scope, empty, children: _*)`.
    pub fn make_xml_elem(
        &mut self,
        span: Span,
        parts: [NodeId; 4],
        empty: bool,
        children: &[NodeId],
    ) -> NodeId {
        let empty = self.ast.node(Tree::Literal { value: Constant::Boolean(empty) });
        let mut args = parts.to_vec();
        args.push(empty);
        if !children.is_empty() {
            let seq = self.make_xml_seq(span, children, true);
            let star = self.type_name("_*");
            let tpt = self.ast.node(Tree::Ident { name: star });
            args.push(self.ast.node(Tree::Typed { expr: seq, tpt }));
        }
        let t = self.xml_new("Elem", &args);
        self.at_span(span, t)
    }
}
