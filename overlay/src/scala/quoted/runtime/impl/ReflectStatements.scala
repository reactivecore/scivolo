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
    def apply(symbol: Symbol): TypeDef = ReflectOps.term(ReflectOps.one("typeDef", ReflectOps.id(symbol))).asInstanceOf[TypeDef]
    def copy(original: Tree)(name: String, rhs: Tree): TypeDef = ReflectOps.copy(original, name, rhs).asInstanceOf[TypeDef]
    def unapply(tdef: TypeDef): (String, Tree) = (tdef.name, tdef.rhs)
  }

  object Import extends ImportModule {
    def apply(expr: Term, selectors: List[Selector]): Import = {
      if (selectors.isEmpty) throw new IllegalArgumentException("Empty selectors")
      ReflectOps.term(ReflectOps.one("import", (expr :: selectors).map(ReflectOps.id)*)).asInstanceOf[Import]
    }
    def copy(original: Tree)(expr: Term, selectors: List[Selector]): Import = refused("Import.copy")
    def unapply(tree: Import): (Term, List[Selector]) = (tree.expr, tree.selectors)
  }

  given SimpleSelectorTypeTest: TypeTest[Selector, SimpleSelector] = selectorTest(s => s.renamed == null && !s.isGiven)
  given RenameSelectorTypeTest: TypeTest[Selector, RenameSelector] = selectorTest(_.renamed != null)
  given OmitSelectorTypeTest: TypeTest[Selector, OmitSelector] = selectorTest(_.renamed == "_")
  given GivenSelectorTypeTest: TypeTest[Selector, GivenSelector] = selectorTest(_.isGiven)

  given SimpleSelectorMethods: SimpleSelectorMethods = methods("SimpleSelectorMethodsImpl")
  given RenameSelectorMethods: RenameSelectorMethods = methods("RenameSelectorMethodsImpl")
  given OmitSelectorMethods: OmitSelectorMethods = methods("OmitSelectorMethodsImpl")
  given GivenSelectorMethods: GivenSelectorMethods = methods("GivenSelectorMethodsImpl")

  object Selector extends SelectorModule

  object SimpleSelector extends SimpleSelectorModule {
    def apply(name: String): SimpleSelector = selector(None, name).asInstanceOf[SimpleSelector]
    def unapply(x: SimpleSelector): Some[String] = Some(x.name)
  }

  object RenameSelector extends RenameSelectorModule {
    def apply(fromName: String, toName: String): RenameSelector =
      selector(None, fromName + "\n" + toName).asInstanceOf[RenameSelector]
    def unapply(x: RenameSelector): (String, String) = (x.fromName, x.toName)
  }

  object OmitSelector extends OmitSelectorModule {
    def apply(name: String): OmitSelector = selector(None, name + "\n_").asInstanceOf[OmitSelector]
    def unapply(x: OmitSelector): Some[String] = Some(SelectorParts(x).imported)
  }

  object GivenSelector extends GivenSelectorModule {
    def apply(bound: Option[TypeTree]): GivenSelector = selector(bound, "").asInstanceOf[GivenSelector]
    def unapply(x: GivenSelector): Some[Option[TypeTree]] = Some(x.bound)
  }

  abstract class SimpleSelectorMethodsImpl extends SimpleSelectorMethods {
    extension (self: SimpleSelector) {
      def name: String = SelectorParts(self).imported
      def namePos: Position = SelectorParts(self).importedPos.asInstanceOf[Position]
    }
  }

  abstract class RenameSelectorMethodsImpl extends RenameSelectorMethods {
    extension (self: RenameSelector) {
      def fromName: String = SelectorParts(self).imported
      def fromPos: Position = SelectorParts(self).importedPos.asInstanceOf[Position]
      def toName: String = SelectorParts(self).renamed
      def toPos: Position = SelectorParts(self).renamedPos.asInstanceOf[Position]
    }
  }

  abstract class OmitSelectorMethodsImpl extends OmitSelectorMethods {
    extension (self: OmitSelector) {
      def name: String = "Ident(" + SelectorParts(self).imported + ")"
      def namePos: Position = SelectorParts(self).importedPos.asInstanceOf[Position]
    }
  }

  abstract class GivenSelectorMethodsImpl extends GivenSelectorMethods {
    extension (self: GivenSelector) def bound: Option[TypeTree] = SelectorParts(self).bound.asInstanceOf[Option[TypeTree]]
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
      def selectors: List[Selector] = ReflectOps.shape(self).from(1).asInstanceOf[List[Selector]]
    }
  }

  abstract class ExportMethodsImpl extends ExportMethods {
    extension (self: Export) {
      def expr: Term = ReflectOps.shape(self).child(0).asInstanceOf[Term]
      def selectors: List[Selector] = ReflectOps.shape(self).from(1).asInstanceOf[List[Selector]]
    }
  }

  abstract class PackageClauseMethodsImpl extends PackageClauseMethods {
    extension (self: PackageClause) {
      def pid: Ref = ReflectOps.shape(self).child(0).asInstanceOf[Ref]
      def stats: List[Tree] = ReflectOps.shape(self).from(1).asInstanceOf[List[Tree]]
    }
  }

  private def test[T](kinds: Int*): T = new KindTest(kinds*).asInstanceOf[T]

  private def selectorTest[T](fits: SelectorParts => Boolean): T = new SelectorTest(fits).asInstanceOf[T]

  /** The selector of `imported` (empty for `given`), renamed to what follows a newline. */
  private def selector(bound: Option[TypeTree], names: String): Any = {
    ReflectOps.run("importSelector", Array(bound.fold(-1)(ReflectOps.id)), names)
    ReflectOps.term(QuoteHost.result(0))
  }

  private def refused(what: String): Nothing = throw new Unsupported(what)

  private def methods[T](name: String): T =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectStatementSupport$ReflectStatements$" + name).asInstanceOf[T]
}
}

/** Import selector `x`: the name it imports, the name it renames that to (`null` for none), and
 *  their positions.
 */
final class SelectorParts(x: Any) {
  private val ids = ReflectOps.shape(x).children

  val imported: String = ReflectOps.shape(ReflectOps.term(ids(0))).name
  val renamed: String = if (ids(1) < 0) null else ReflectOps.shape(ReflectOps.term(ids(1))).name

  def isGiven: Boolean = imported.isEmpty

  def bound: Option[Any] = ReflectOps.opt(ids(2))

  def importedPos: PositionImpl = ReflectOps.position("pos", ids(0))

  def renamedPos: PositionImpl = ReflectOps.position("pos", ids(1))
}

/** The type test of import selectors whose parts `fits` accepts. */
final class SelectorTest(fits: SelectorParts => Boolean) extends TypeTest[Any, Any] {
  def unapply(x: Any): Option[x.type & Any] = if (fits(new SelectorParts(x))) Some(x) else None
}
