//! The JDK classes Scala Native defines under other names (nscplugin's `NirGenName`), entered
//! under their JDK names.

/// Each class's qualified name where Scala Native defines it, and its JDK name.
pub const RENAMED: [(&str, &str); 9] = [
    ("scala.scalanative.runtime._Class", "java.lang.Class"),
    ("scala.scalanative.runtime._Object", "java.lang.Object"),
    ("java.lang._Cloneable", "java.lang.Cloneable"),
    ("java.lang._Comparable", "java.lang.Comparable"),
    ("java.lang._Enum", "java.lang.Enum"),
    ("java.lang._NullPointerException", "java.lang.NullPointerException"),
    ("java.lang._String", "java.lang.String"),
    ("java.lang.annotation._Retention", "java.lang.annotation.Retention"),
    ("java.io._Serializable", "java.io.Serializable"),
];

/// The JDK package and name of top-level `name` in package `pkg`, a module class's `$` kept,
/// if Scala Native defines it under another name.
pub fn jdk_name(pkg: &str, name: &str) -> Option<(&'static str, String)> {
    if !name.starts_with('_') {
        return None;
    }
    let module = super::module_name(name);
    let full = format!("{pkg}.{}", module.unwrap_or(name));
    let (_, jdk) = RENAMED.iter().find(|(defined, _)| *defined == full)?;
    let (jdk_pkg, jdk_simple) = jdk.rsplit_once('.')?;
    let jdk_name =
        if module.is_some() { super::module_class_name(jdk_simple) } else { jdk_simple.to_owned() };
    Some((jdk_pkg, jdk_name))
}

/// The qualified name Scala Native defines JDK class `jdk` under, if it is renamed.
pub fn defined_name(jdk: &str) -> Option<&'static str> {
    RENAMED.iter().find(|(_, j)| *j == jdk).map(|(defined, _)| *defined)
}

/// The JDK name of member `name` of class `owner` (qualified, JDK name), if Scala Native gives
/// it another: `java.lang.Object`'s methods are `__hashCode` and so on.
pub fn jdk_member(owner: &str, name: &str) -> Option<String> {
    (owner == "java.lang.Object").then(|| name.strip_prefix("__").map(str::to_owned)).flatten()
}

#[cfg(test)]
mod tests {
    use super::{defined_name, jdk_member, jdk_name};

    #[test]
    fn maps_scala_native_names_to_the_jdk() {
        assert_eq!(jdk_name("java.lang", "_String"), Some(("java.lang", "String".into())));
        assert_eq!(jdk_name("java.lang", "_String$"), Some(("java.lang", "String$".into())));
        assert_eq!(jdk_name("scala.scalanative.runtime", "_Object"), Some(("java.lang", "Object".into())));
        assert_eq!(jdk_name("java.lang", "String"), None);
        assert_eq!(jdk_member("java.lang.Object", "__hashCode").as_deref(), Some("hashCode"));
        assert_eq!(jdk_member("java.lang.String", "__hashCode"), None);
        assert_eq!(defined_name("java.lang.String"), Some("java.lang._String"));
    }
}
