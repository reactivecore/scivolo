//! The members of `MethodHandles`, `MethodHandles.Lookup` and `VarHandle` that lazy values call,
//! as the class files of JDK 9 and later declare them, for a compilation without a readable JDK.

use super::{ClassInfo, MemberInfo};
use crate::{
    ACC_ABSTRACT, ACC_FINAL, ACC_NATIVE, ACC_PUBLIC, ACC_STATIC, ACC_SUPER, ACC_VARARGS, InnerClass,
};

const METHOD_HANDLES: &str = "java/lang/invoke/MethodHandles";
const LOOKUP: &str = "java/lang/invoke/MethodHandles$Lookup";
const VAR_HANDLE: &str = "java/lang/invoke/VarHandle";

/// Describes class `name` if it is one of the three classes.
pub fn class(name: &str) -> Option<ClassInfo> {
    let lookup = InnerClass {
        inner: LOOKUP.into(),
        outer: Some(METHOD_HANDLES.into()),
        name: Some("Lookup".into()),
        flags: ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
    };
    Some(match name {
        METHOD_HANDLES => ClassInfo {
            inner_classes: vec![lookup],
            methods: vec![method(
                ACC_PUBLIC | ACC_STATIC,
                "lookup",
                "()Ljava/lang/invoke/MethodHandles$Lookup;",
                None,
            )],
            ..info(ACC_PUBLIC | ACC_FINAL | ACC_SUPER, name)
        },
        LOOKUP => ClassInfo {
            inner_classes: vec![lookup],
            methods: vec![method(
                ACC_PUBLIC,
                "findVarHandle",
                "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/invoke/VarHandle;",
                Some(
                    "(Ljava/lang/Class<*>;Ljava/lang/String;Ljava/lang/Class<*>;)Ljava/lang/invoke/VarHandle;",
                ),
            )],
            ..info(ACC_PUBLIC | ACC_FINAL | ACC_SUPER, name)
        },
        VAR_HANDLE => ClassInfo {
            methods: vec![method(
                ACC_PUBLIC | ACC_FINAL | ACC_NATIVE | ACC_VARARGS,
                "compareAndSet",
                "([Ljava/lang/Object;)Z",
                None,
            )],
            ..info(ACC_PUBLIC | ACC_ABSTRACT | ACC_SUPER, name)
        },
        _ => return None,
    })
}

fn info(access: u16, name: &str) -> ClassInfo {
    ClassInfo {
        access,
        name: name.into(),
        superclass: Some("java/lang/Object".into()),
        interfaces: Vec::new(),
        fields: Vec::new(),
        methods: Vec::new(),
        signature: None,
        annotations: Vec::new(),
        inner_classes: Vec::new(),
        permitted: Vec::new(),
        scala: None,
    }
}

fn method(access: u16, name: &str, descriptor: &str, signature: Option<&str>) -> MemberInfo {
    MemberInfo {
        access,
        name: name.into(),
        descriptor: descriptor.into(),
        signature: signature.map(str::to_owned),
        constant: None,
        annotation_default: false,
        param_names: Vec::new(),
    }
}
