// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nir/src/main/scala/scala/scalanative/nir/Sig.scala
//   nir/src/main/scala/scala/scalanative/nir/Unmangle.scala
//! Parts of mangled signatures: names, types, the proxy signature of a method.

use super::Demangler;

/// The reflective proxy signature `P<name><params>E` of method signature
/// `D<name><params><result>E<scope>`, as Scala Native's `Sig.toProxy` builds it.
pub fn to_proxy(s: &str) -> Option<String> {
    let mut d = Demangler::new(s);
    if d.next()? != 'D' {
        return None;
    }
    d.ident()?;
    let mut result_start = None;
    while d.peek()? != 'E' {
        result_start = Some(d.pos);
        d.ty()?;
    }
    let name_and_params: String = d.chars[1..result_start?].iter().collect();
    Some(format!("P{name_and_params}E"))
}

/// The name and the mangled parameter and result types, result last, of public instance
/// method signature `D<name><params><result>EO`.
pub fn method(s: &str) -> Option<(String, Vec<String>)> {
    let mut d = Demangler::new(s);
    if d.next()? != 'D' {
        return None;
    }
    let name = d.ident()?;
    let mut tys = Vec::new();
    while d.peek()? != 'E' {
        let start = d.pos;
        d.ty()?;
        tys.push(d.chars[start..d.pos].iter().collect());
    }
    (d.chars[d.pos + 1..] == ['O'] && !tys.is_empty()).then_some((name, tys))
}

pub fn extern_name(s: &str) -> Option<String> {
    let mut d = Demangler::new(s);
    (d.next()? == 'C').then(|| d.ident())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_method_signatures() {
        let (name, tys) = method("D5applyL16java.lang.ObjectiEO").unwrap();
        assert_eq!((name.as_str(), tys), ("apply", vec!["L16java.lang.Object".to_owned(), "i".to_owned()]));
        assert!(method("D5applyiEo").is_none(), "static");
        assert!(method("D3fooiEPT3Foo").is_none(), "private");
        assert_eq!(to_proxy("D5applyL16java.lang.ObjectiEO").as_deref(), Some("P5applyL16java.lang.ObjectE"));
    }
}
