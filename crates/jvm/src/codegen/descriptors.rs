// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeUtils.scala
//! The internal names and descriptors of classes, fields and methods.

use lower::jdk_class_name;
use tasty::{Erased, SymbolId, Universe};

/// Builds the internal name of the class of binary name `binary`, as the JDK names it.
pub fn internal(binary: &str) -> String {
    match binary {
        "scala.Any" | "scala.AnyRef" => "java/lang/Object".into(),
        _ => jdk_class_name(binary).replace('.', "/"),
    }
}

/// Builds the field descriptor `L…;` of the class of internal name `internal`.
pub fn class_descriptor(internal: &str) -> String {
    format!("L{internal};")
}

/// Builds the field descriptor of erased type `e`.
pub fn descriptor(e: &Erased) -> String {
    match e {
        Erased::Prim(c) => match c {
            'z' => "Z",
            'c' => "C",
            'b' => "B",
            's' => "S",
            'i' => "I",
            'j' => "J",
            'f' => "F",
            _ => "D",
        }
        .into(),
        Erased::Unit => "V".into(),
        Erased::Nothing => "Lscala/runtime/Nothing$;".into(),
        Erased::Null => "Lscala/runtime/Null$;".into(),
        Erased::Class(name) | Erased::Value(name, _) => class_descriptor(&internal(name)),
        Erased::Array(elem) => format!("[{}", descriptor(elem)),
    }
}

/// Builds the internal name of a class or array type of erased type `e`, as `ldc` and `new` take
/// it.
pub fn class_internal(e: &Erased) -> String {
    match e {
        Erased::Class(name) | Erased::Value(name, _) => internal(name),
        Erased::Nothing => "scala/runtime/Nothing$".into(),
        Erased::Null => "scala/runtime/Null$".into(),
        e => descriptor(e),
    }
}

/// Determines the class whose `TYPE` field holds the class of primitive erased type `e`.
pub fn primitive_box(e: &Erased) -> Option<&'static str> {
    let Erased::Prim(c) = e else { return (*e == Erased::Unit).then_some("java/lang/Void") };
    Some(match c {
        'z' => "java/lang/Boolean",
        'c' => "java/lang/Character",
        'b' => "java/lang/Byte",
        's' => "java/lang/Short",
        'i' => "java/lang/Integer",
        'j' => "java/lang/Long",
        'f' => "java/lang/Float",
        _ => "java/lang/Double",
    })
}

/// Builds the method descriptor `(params)result` of parameter descriptors `params` and result
/// descriptor `result`.
pub fn method_type(params: &str, result: &str) -> String {
    format!("({params}){result}")
}

/// Builds the descriptor of a static method taking an instance of the class of internal name
/// `owner` before the parameters of method descriptor `desc`.
pub fn with_receiver(owner: &str, desc: &str) -> String {
    format!("({}{}", class_descriptor(owner), &desc[1..])
}

/// Builds the method descriptor of method `m`; `V` as the result of a constructor.
pub fn method_descriptor(u: &mut Universe, m: SymbolId) -> String {
    let (params, result) = u.erased_method(m);
    let result = if u.is_constructor(m) { Erased::Unit } else { result };
    let params: String = params.iter().map(descriptor).collect();
    method_type(&params, &descriptor(&result))
}
