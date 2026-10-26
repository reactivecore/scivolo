// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.reflect.TypeTest

/** The type definitions, imports, exports and package clauses of `quotes.reflect`. */
trait ReflectStatementSupport { this: QuotesImpl =>

trait ReflectStatements extends reflectModule { self: reflect.type =>

  given TypeDefTypeTest: TypeTest[Tree, TypeDef] = test(16)
  given ValOrDefDefTypeTest: TypeTest[Tree, ValOrDefDef] = test(12, 15)
  given ImportTypeTest: TypeTest[Tree, Import] = test(17)
  given ExportTypeTest: TypeTest[Tree, Export] = test(48)
  given PackageClauseTypeTest: TypeTest[Tree, PackageClause] = test(49)

  given TypeDefMethods: TypeDefMethods = methods("TypeDefMethodsImpl")
  given ValOrDefDefMethods: ValOrDefDefMethods = methods("ValOrDefDefMethodsImpl")
  given ImportMethods: ImportMethods = methods("ImportMethodsImpl")
  given ExportMethods: ExportMethods = methods("ExportMethodsImpl")
  given PackageClauseMethods: PackageClauseMethods = methods("PackageClauseMethodsImpl")

  object TypeDef extends TypeDefModule {
    def apply(symbol: Symbol): TypeDef = refused("TypeDef.apply")
    def copy(original: Tree)(name: String, rhs: Tree): TypeDef = ReflectOps.copy(original, name, rhs).asInstanceOf[TypeDef]
    def unapply(tdef: TypeDef): (String, Tree) = (tdef.name, tdef.rhs)
  }

  /** Imports and exports keep their selectors, which are refused. */
  object Import extends ImportModule {
    def apply(expr: Term, selectors: List[Selector]): Import = refused("Import.apply")
    def copy(original: Tree)(expr: Term, selectors: List[Selector]): Import = refused("Import.copy")
    def unapply(tree: Import): (Term, List[Selector]) = (tree.expr, tree.selectors)
  }

  object Export extends ExportModule {
    def unapply(tree: Export): (Term, List[Selector]) = (tree.expr, tree.selectors)
  }

  object PackageClause extends PackageClauseModule {
    def apply(pid: Ref, stats: List[Tree]): PackageClause = refused("PackageClause.apply")
    def copy(original: Tree)(pid: Ref, stats: List[Tree]): PackageClause =
      ReflectOps.copy(original, null, pid, stats).asInstanceOf[PackageClause]
    def unapply(tree: PackageClause): (Ref, List[Tree]) = (tree.pid, tree.stats)
  }

  abstract class TypeDefMethodsImpl extends TypeDefMethods {
    extension (self: TypeDef) def rhs: Tree = ReflectOps.shape(self).child(0).asInstanceOf[Tree]
  }

  abstract class ValOrDefDefMethodsImpl extends ValOrDefDefMethods {
    extension (self: ValOrDefDef) {
      def tpt: TypeTree = ReflectOps.shape(self).child(0).asInstanceOf[TypeTree]
      def rhs: Option[Term] = ReflectOps.opt(ReflectOps.shape(self).children(1)).asInstanceOf[Option[Term]]
    }
  }

  abstract class ImportMethodsImpl extends ImportMethods {
    extension (self: Import) {
      def expr: Term = ReflectOps.shape(self).child(0).asInstanceOf[Term]
      def selectors: List[Selector] = throw new Unsupported("Import.selectors")
    }
  }

  abstract class ExportMethodsImpl extends ExportMethods {
    extension (self: Export) {
      def expr: Term = ReflectOps.shape(self).child(0).asInstanceOf[Term]
      def selectors: List[Selector] = throw new Unsupported("Export.selectors")
    }
  }

  abstract class PackageClauseMethodsImpl extends PackageClauseMethods {
    extension (self: PackageClause) {
      def pid: Ref = ReflectOps.shape(self).child(0).asInstanceOf[Ref]
      def stats: List[Tree] = ReflectOps.shape(self).from(1).asInstanceOf[List[Tree]]
    }
  }

  private def test[T](kinds: Int*): T = new KindTest(kinds*).asInstanceOf[T]

  private def refused(what: String): Nothing = throw new Unsupported(what)

  private def methods[T](name: String): T =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectStatementSupport$ReflectStatements$" + name).asInstanceOf[T]
}
}
