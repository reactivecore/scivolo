// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BackendUtils.scala
//! A class's `$deserializeLambda$` (dotty's `addLambdaDeserialize`).

use super::Gen;
use super::closures::BOOTSTRAP;
use classfile::{ACC_PRIVATE, ACC_STATIC, ACC_SYNTHETIC, ClassFile, Code, Constant, Handle, Label};

/// The most method handles one `LambdaDeserialize` call site takes.
const GROUP: usize = 255 - 1 - 3;
const SERIALIZED_LAMBDA: &str = "(Ljava/lang/invoke/SerializedLambda;)Ljava/lang/Object;";

impl Gen<'_, '_> {
    /// Adds `$deserializeLambda$` to `cf` if its methods make serializable lambdas: a call site
    /// per group of lambdas, each but the last trying the next on an `IllegalArgumentException`.
    pub fn deserialize_lambda(&mut self, cf: &mut ClassFile) {
        let lambdas = std::mem::take(&mut self.lambdas);
        if lambdas.is_empty() {
            return;
        }
        let groups: Vec<&[Handle]> = lambdas.chunks(GROUP).collect();
        let mut code = Code::new(1);
        let labels: Vec<Label> = groups.iter().map(|_| code.new_label()).collect();
        let illegal = cf.pool.class("java/lang/IllegalArgumentException");
        for pair in labels.windows(2) {
            code.protect(pair[0], pair[1], pair[1], illegal);
        }
        let d = format!("({BOOTSTRAP}[Ljava/lang/invoke/MethodHandle;)Ljava/lang/invoke/CallSite;");
        let bootstrap = Handle::static_method("scala/runtime/LambdaDeserialize", "bootstrap", &d);
        for (i, (group, &label)) in groups.iter().zip(&labels).enumerate() {
            code.place(label);
            code.set_stack(i32::from(i > 0));
            code.load("L", 0);
            let args: Vec<Constant> = group.iter().map(Constant::Handle).collect();
            code.invoke_dynamic(&mut cf.pool, ("lambdaDeserialize", SERIALIZED_LAMBDA), &bootstrap, &args);
            code.ret("L");
        }
        let flags = ACC_PRIVATE | ACC_STATIC | ACC_SYNTHETIC;
        cf.method(flags, "$deserializeLambda$", SERIALIZED_LAMBDA, Some(code), &mut |_, _| None);
    }
}
