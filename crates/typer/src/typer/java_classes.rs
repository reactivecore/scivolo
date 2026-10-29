//! The Java classes of dotty's class path the universe may lack: those of the JDK's `java.lang`
//! (17 to 25) and the Java-defined ones of the Scala library, nested ones aside; whether a class
//! or member is a Java one; the JDK's functional interfaces.

use super::Typer;
use tasty::SymbolId;

impl Typer<'_> {
    /// Whether `s` is a member of a `java.*` or `javax.*` class or a Java class of the Scala
    /// library, its parameter names unknown.
    pub fn is_java_member(&mut self, s: SymbolId) -> bool {
        self.u.symbol(s).owner.is_some_and(|owner| self.is_java_class_name(owner))
    }

    /// Whether `c`, from TASTy, is named as a `java.*` or `javax.*` class or a Java class of the
    /// Scala library; a class of the unit, or of javalib when compiling it, is a Scala one.
    pub fn is_java_class_name(&mut self, c: SymbolId) -> bool {
        if !self.u.may_be_java(c) && self.u.symbol(c).kind != tasty::Kind::Package {
            return false;
        }
        let name = self.u.full_name(c);
        let library_java =
            name.rsplit_once('.').is_some_and(|(pkg, simple)| java_classes(pkg).any(|c| c == simple));
        name.starts_with("java.") || name.starts_with("javax.") || library_java
    }
}

/// Whether the JDK class named `full` is annotated `@FunctionalInterface`.
pub fn is_functional_interface(full: &str) -> bool {
    full.starts_with("java.util.function.")
        || matches!(full, "java.lang.Runnable" | "java.util.Comparator" | "java.util.concurrent.Callable")
}

/// The names of the Java classes of package `package` the universe may lack.
pub fn java_classes(package: &str) -> impl Iterator<Item = &'static str> {
    let classes = match package {
        "java.lang" => JAVA_LANG,
        "scala.annotation.internal" => "TASTYLongSignature TASTYSignature",
        "scala.collection.concurrent" => "BasicNode CNodeBase Gen INodeBase MainNode",
        "scala.math" => "ScalaNumber",
        "scala.reflect" => "ScalaLongSignature ScalaSignature",
        "scala.runtime" => SCALA_RUNTIME,
        "scala.runtime.function" => SCALA_RUNTIME_FUNCTION,
        _ => "",
    };
    classes.split_whitespace()
}

const JAVA_LANG: &str = "AbstractMethodError AbstractStringBuilder Appendable ApplicationShutdownHooks \
    ArithmeticException ArrayIndexOutOfBoundsException ArrayStoreException AssertionError \
    AssertionStatusDirectives AutoCloseable BaseVirtualThread Boolean BootstrapMethodError Byte \
    CharSequence Character CharacterData CharacterData00 CharacterData01 CharacterData02 \
    CharacterData03 CharacterData0E CharacterDataLatin1 CharacterDataPrivateUse CharacterDataUndefined \
    CharacterName Class ClassCastException ClassCircularityError ClassFormatError ClassFrameInfo \
    ClassLoader ClassNotFoundException ClassValue CloneNotSupportedException Cloneable Comparable \
    Compiler CompoundEnumeration ConditionalSpecialCasing Deprecated Double Enum \
    EnumConstantNotPresentException Error Exception ExceptionInInitializerError FdLibm Float \
    FunctionalInterface IO IllegalAccessError IllegalAccessException IllegalArgumentException \
    IllegalCallerException IllegalMonitorStateException IllegalStateException \
    IllegalThreadStateException IncompatibleClassChangeError IndexOutOfBoundsException \
    InheritableThreadLocal InstantiationError InstantiationException Integer InternalError \
    InterruptedException Iterable LayerInstantiationException LinkageError LiveStackFrame \
    LiveStackFrameInfo Long MatchException Math Module ModuleLayer NamedPackage \
    NegativeArraySizeException NoClassDefFoundError NoSuchFieldError NoSuchFieldException \
    NoSuchMethodError NoSuchMethodException NullPointerException Number NumberFormatException Object \
    OutOfMemoryError Override Package PinnedThreadPrinter Process ProcessBuilder ProcessEnvironment \
    ProcessHandle ProcessHandleImpl ProcessImpl PublicMethods Readable Record \
    ReflectiveOperationException Runnable Runtime RuntimeException RuntimePermission SafeVarargs \
    ScopedValue SecurityException SecurityManager Short Shutdown StableValue StackFrameInfo \
    StackOverflowError StackStreamFactory StackTraceElement StackWalker StrictMath String StringBuffer \
    StringBuilder StringCoding StringConcatHelper StringIndexOutOfBoundsException StringLatin1 \
    StringTemplate StringUTF16 SuppressWarnings System Terminator Thread ThreadBuilders ThreadDeath \
    ThreadGroup ThreadLocal Throwable TypeNotPresentException UnknownError UnsatisfiedLinkError \
    UnsupportedClassVersionError UnsupportedOperationException VerifyError VersionProps \
    VirtualMachineError VirtualThread Void WeakPairMap WrongThreadException";

const SCALA_RUNTIME: &str = "BooleanRef BoxedUnit BoxesRunTime ByteRef CharRef DoubleRef FloatRef IntRef \
    LongRef MethodHandleConstants ObjectRef ShortRef Static Statics SymbolLiteral VolatileBooleanRef \
    VolatileByteRef VolatileCharRef VolatileDoubleRef VolatileFloatRef VolatileIntRef VolatileLongRef \
    VolatileObjectRef VolatileShortRef";

const SCALA_RUNTIME_FUNCTION: &str = "JProcedure0 JProcedure1 JProcedure2 JProcedure3 JProcedure4 \
    JProcedure5 JProcedure6 JProcedure7 JProcedure8 JProcedure9 JProcedure10 JProcedure11 JProcedure12 \
    JProcedure13 JProcedure14 JProcedure15 JProcedure16 JProcedure17 JProcedure18 JProcedure19 \
    JProcedure20 JProcedure21 JProcedure22";
