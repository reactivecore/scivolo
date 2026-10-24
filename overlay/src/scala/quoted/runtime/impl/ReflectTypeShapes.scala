// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.reflect.TypeTest

/** The type tests and extractors of `TypeRepr`s, over the frontend's type kinds
 *  (`TypeShapes.kind`).
 */
trait ReflectTypeShapeSupport { this: QuotesImpl =>

trait ReflectTypeShapes extends reflectModule { self: reflect.type =>

  given NamedTypeTypeTest: TypeTest[TypeRepr, NamedType] = test(1, 2)
  given TypeRefTypeTest: TypeTest[TypeRepr, TypeRef] = test(1)
  given TermRefTypeTest: TypeTest[TypeRepr, TermRef] = test(2)
  given AppliedTypeTypeTest: TypeTest[TypeRepr, AppliedType] = test(3)
  given AnnotatedTypeTypeTest: TypeTest[TypeRepr, AnnotatedType] = test(4)

  given NamedTypeMethods: NamedTypeMethods = methods("NamedTypeMethodsImpl")
  given TypeRefMethods: TypeRefMethods = methods("TypeRefMethodsImpl")
  given AppliedTypeMethods: AppliedTypeMethods = methods("AppliedTypeMethodsImpl")
  given AnnotatedTypeMethods: AnnotatedTypeMethods = methods("AnnotatedTypeMethodsImpl")

  object TypeRef extends TypeRefModule {
    def unapply(x: TypeRef): (TypeRepr, String) =
      (TypeShapes.part(x, 0).asInstanceOf[TypeRepr], TypeShapes.name(x))
  }

  object TermRef extends TermRefModule {
    def apply(qual: TypeRepr, name: String): TermRef = {
      ReflectOps.run("termRef", Array(ReflectOps.id(qual)), name)
      QuoteHost.tpe(QuoteHost.result(0)).asInstanceOf[TermRef]
    }
    def unapply(x: TermRef): (TypeRepr, String) =
      (TypeShapes.part(x, 0).asInstanceOf[TypeRepr], TypeShapes.name(x))
  }

  object AppliedType extends AppliedTypeModule {
    def apply(tycon: TypeRepr, args: List[TypeRepr]): AppliedType = TypeShapes.appliedTo(tycon, args).asInstanceOf[AppliedType]
    def unapply(x: AppliedType): (TypeRepr, List[TypeRepr]) = {
      val parts = TypeShapes.parts(x)
      (parts.head.asInstanceOf[TypeRepr], parts.tail.asInstanceOf[List[TypeRepr]])
    }
  }

  /** A method type of parameter and result types that do not refer to it. */
  object MethodType extends MethodTypeModule {
    def apply(paramNames: List[String])(paramInfosExp: MethodType => List[TypeRepr], resultTypeExp: MethodType => TypeRepr): MethodType = {
      val unknown = QuoteHost.tpe(Int.MaxValue).asInstanceOf[MethodType]
      val types = paramInfosExp(unknown) ::: List(resultTypeExp(unknown))
      ReflectOps.run("methodType", types.map(ReflectOps.id).toArray, paramNames.mkString("\u0000"))
      QuoteHost.tpe(QuoteHost.result(0)).asInstanceOf[MethodType]
    }
    def apply(kind: MethodTypeKind)(paramNames: List[String])(paramInfosExp: MethodType => List[TypeRepr],
        resultTypeExp: MethodType => TypeRepr): MethodType = throw new Unsupported("MethodType.apply of a kind")
    def unapply(x: MethodType): (List[String], List[TypeRepr], TypeRepr) = throw new Unsupported("MethodType.unapply")
  }

  object ByNameType extends ByNameTypeModule {
    def apply(underlying: TypeRepr): TypeRepr =
      QuoteHost.tpe(ReflectOps.one("byName", ReflectOps.id(underlying))).asInstanceOf[TypeRepr]
    def unapply(x: ByNameType): Some[TypeRepr] = throw new Unsupported("ByNameType.unapply")
  }

  object AnnotatedType extends AnnotatedTypeModule {
    def apply(underlying: TypeRepr, annot: Term): AnnotatedType = throw new Unsupported("AnnotatedType.apply")
    def unapply(x: AnnotatedType): (TypeRepr, Term) =
      (TypeShapes.part(x, 0).asInstanceOf[TypeRepr], TypeShapes.annotation(x).asInstanceOf[Term])
  }

  abstract class NamedTypeMethodsImpl extends NamedTypeMethods {
    extension (self: NamedType) {
      def qualifier: TypeRepr = TypeShapes.part(self, 0).asInstanceOf[TypeRepr]
      def name: String = TypeShapes.name(self)
    }
  }

  abstract class TypeRefMethodsImpl extends TypeRefMethods {
    extension (self: TypeRef) {
      def isOpaqueAlias: Boolean = throw new Unsupported("TypeRef.isOpaqueAlias")
      def translucentSuperType: TypeRepr = throw new Unsupported("TypeRef.translucentSuperType")
    }
  }

  abstract class AppliedTypeMethodsImpl extends AppliedTypeMethods {
    extension (self: AppliedType) {
      def tycon: TypeRepr = TypeShapes.part(self, 0).asInstanceOf[TypeRepr]
      def args: List[TypeRepr] = TypeShapes.parts(self).tail.asInstanceOf[List[TypeRepr]]
    }
  }

  abstract class AnnotatedTypeMethodsImpl extends AnnotatedTypeMethods {
    extension (self: AnnotatedType) {
      def underlying: TypeRepr = TypeShapes.part(self, 0).asInstanceOf[TypeRepr]
      def annotation: Term = TypeShapes.annotation(self).asInstanceOf[Term]
    }
  }

  private def test[T](kinds: Int*): T = new TypeKindTest(kinds*).asInstanceOf[T]

  private def methods[T](name: String): T =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectTypeShapeSupport$ReflectTypeShapes$" + name).asInstanceOf[T]
}
}

/** Type kinds: 1 `TypeRef`, 2 `TermRef`, 3 `AppliedType`, 4 `AnnotatedType`, 0 another type. */
object TypeShapes {
  def kind(t: Any): Int = ReflectOps.one("typeKind", ReflectOps.id(t))

  def parts(t: Any): List[Any] = ReflectOps.results("typeParts", ReflectOps.id(t)).map(QuoteHost.tpe)

  def part(t: Any, i: Int): Any = parts(t)(i)

  def name(t: Any): String = ReflectOps.text("typeName", ReflectOps.id(t))

  /** The annotation of annotated type `t`: an instance creation of the annotation's class. */
  def annotation(t: Any): Any = ReflectOps.term(ReflectOps.one("annotation", ReflectOps.id(t)))

  def appliedTo(tycon: Any, args: List[Any]): Any =
    QuoteHost.tpe(ReflectOps.one("appliedTo", (tycon :: args).map(ReflectOps.id)*))
}

/** The type test of types of kinds `kinds`. */
final class TypeKindTest(kinds: Int*) extends TypeTest[Any, Any] {
  def unapply(x: Any): Option[x.type & Any] = if (kinds.contains(TypeShapes.kind(x))) Some(x) else None
}
