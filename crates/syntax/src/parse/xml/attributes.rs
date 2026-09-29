//! Attributes: reading them, and the trees of their values with references.

use super::char_ref::char_ref_units;
use super::lexical::{SU, is_name_start};
use super::{Abort, XmlResult};
use crate::ast::constant::Constant;
use crate::ast::{NodeId, Span, Tree};
use crate::names::to_utf16;
use crate::parse::Parser;
use rustc_hash::FxHashMap;

impl Parser<'_> {
    /// `{S Name Eq AttValue}`, in the order written; a repeated name keeps its first place.
    pub fn x_attributes(&mut self) -> XmlResult<Vec<(String, NodeId)>> {
        let mut attrs: Vec<(String, NodeId)> = Vec::new();
        let mut index: FxHashMap<String, usize> = FxHashMap::default();
        while is_name_start(self.ch()) {
            let start = self.xml_offset();
            let key = self.x_name()?;
            self.x_eq()?;
            let mid = self.xml_offset();
            let value = match self.ch() {
                q @ ('"' | '\'') => {
                    self.xml_next();
                    let text = self.x_attribute_value(q)?;
                    let span = Span::new(start, self.xml_offset(), mid);
                    match self.xml_attribute(span, &text) {
                        Some(t) => t,
                        None => {
                            let o = self.offset();
                            let t = self.error_term_tree(o);
                            self.xml_error("error parsing attribute value");
                            t
                        }
                    }
                }
                '{' => {
                    self.xml_next();
                    self.x_embedded_expr()
                }
                SU => return Err(Abort::Truncated),
                _ => {
                    let lit = to_utf16("<syntax-error>").into_boxed_slice();
                    let t = self.ast.node(Tree::Literal { value: Constant::String(lit) });
                    self.xml_error("' or \" delimited attribute value or '{' scala-expr '}' expected");
                    t
                }
            };
            match index.get(&key) {
                Some(&i) => {
                    self.xml_error(&format!("attribute {key} may only be defined once"));
                    attrs[i].1 = value;
                }
                None => {
                    index.insert(key.clone(), attrs.len());
                    attrs.push((key, value));
                }
            }
            if self.ch() != '/' && self.ch() != '>' {
                self.x_space()?;
            }
        }
        Ok(attrs)
    }

    /// The trees of attribute value `text`: texts and entity references, `Nil` if empty, a
    /// sequence if several; `None` for a malformed reference.
    fn xml_attribute(&mut self, span: Span, text: &str) -> Option<NodeId> {
        let ts = self.attribute_value(span, text)?;
        Some(match ts[..] {
            [] => {
                let t = self.ast.node(Tree::TypedSplice { what: "Nil" });
                self.ast.with_span(t, span.to_synthetic());
                t
            }
            [t] => t,
            _ => self.make_xml_seq(span, &ts, true),
        })
    }

    /// dotty's `Utility.parseAttributeValue`.
    fn attribute_value(&mut self, span: Span, text: &str) -> Option<Vec<NodeId>> {
        let mut units: Vec<u16> = Vec::new();
        let mut nodes = Vec::new();
        let mut it = to_utf16(text).into_iter();
        while let Some(u) = it.next() {
            if u != b'&' as u16 {
                units.push(u);
                continue;
            }
            let u = it.next()?;
            if u == b'#' as u16 {
                units.extend(char_ref_units(it.next()?, &mut it)?);
                continue;
            }
            let mut name = vec![u];
            loop {
                match it.next()? {
                    u if u == b';' as u16 => break,
                    u => name.push(u),
                }
            }
            let name = String::from_utf16_lossy(&name);
            match unescape(&name) {
                Some(c) => units.push(c as u16),
                None => {
                    if !units.is_empty() {
                        nodes.push(self.xml_text_units(span, std::mem::take(&mut units)));
                    }
                    nodes.push(self.entity_ref(span, &name));
                }
            }
        }
        if !units.is_empty() {
            nodes.push(self.xml_text_units(span, units));
        }
        Some(nodes)
    }
}

fn unescape(name: &str) -> Option<char> {
    Some(match name {
        "lt" => '<',
        "gt" => '>',
        "amp" => '&',
        "quot" => '"',
        "apos" => '\'',
        _ => return None,
    })
}
