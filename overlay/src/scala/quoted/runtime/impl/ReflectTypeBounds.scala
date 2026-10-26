// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.reflect.TypeTest

/** The bounds, wildcards, by-name, lambda, binding, block and match type trees and the type
 *  cases of `quotes.reflect`; match type trees are refused.
 */
trait ReflectTypeBoundSupport { this: QuotesImpl =>

trait ReflectTypeBounds extends reflectModule { self: reflect.type =>

  given MatchTypeTreeTypeTest: TypeTest[Tree, MatchTypeTree] = test()
  given ByNameTypeTest: TypeTest[Tree, ByName] = test(44)
  given LambdaTypeTreeTypeTest: TypeTest[Tree, LambdaTypeTree] = test(45)
  given TypeBindTypeTest: TypeTest[Tree, TypeBind] = test(33)
  given TypeBlockTypeTest: TypeTest[Tree, TypeBlock] = test(50)
  given TypeBoundsTreeTypeTest: TypeTest[Tree, TypeBoundsTree] = test(46)
  given WildcardTypeTreeTypeTest: TypeTest[Tree, WildcardTypeTree] = test(47)
  given TypeCaseDefTypeTest: TypeTest[Tree, TypeCaseDef] = test(31)
  given TypeCaseDefMethods: TypeCaseDefMethods = methods("TypeCaseDefMethodsImpl")

  given MatchTypeTreeMethods: MatchTypeTreeMethods = methods("MatchTypeTreeMethodsImpl")
  given ByNameMethods: ByNameMethods = methods("ByNameMethodsImpl")
  given LambdaTypeTreeMethods: LambdaTypeTreeMethods = methods("LambdaTypeTreeMethodsImpl")
  given TypeBindMethods: TypeBindMethods = methods("TypeBindMethodsImpl")
  given TypeBlockMethods: TypeBlockMethods = methods("TypeBlockMethodsImpl")
  given TypeBoundsTreeMethods: TypeBoundsTreeMethods = methods("TypeBoundsTreeMethodsImpl")
  given WildcardTypeTreeMethods: WildcardTypeTreeMethods = methods("WildcardTypeTreeMethodsImpl")

  object MatchTypeTree extends MatchTypeTreeModule {
    def apply(bound: Option[TypeTree], selector: TypeTree, cases: List[TypeCaseDef]): MatchTypeTree =
      refused("MatchTypeTree.apply")
    def copy(original: Tree)(bound: Option[TypeTree], selector: TypeTree, cases: List[TypeCaseDef]): MatchTypeTree =
      refused("MatchTypeTree.copy")
    def unapply(x: MatchTypeTree): (Option[TypeTree], TypeTree, List[TypeCaseDef]) = refused("MatchTypeTree.unapply")
  }

  object ByName extends ByNameModule {
    def apply(result: TypeTree): ByName = refused("ByName.apply")
    def copy(original: Tree)(result: TypeTree): ByName = copied(original, null, result)
    def unapply(x: ByName): Some[TypeTree] = Some(x.result)
  }

  object LambdaTypeTree extends LambdaTypeTreeModule {
    def apply(tparams: List[TypeDef], body: Tree): LambdaTypeTree = refused("LambdaTypeTree.apply")
    def copy(original: Tree)(tparams: List[TypeDef], body: Tree): LambdaTypeTree = copied(original, null, body, tparams)
    def unapply(tree: LambdaTypeTree): (List[TypeDef], Tree) = (tree.tparams, tree.body)
  }

  object TypeBind extends TypeBindModule {
    def copy(original: Tree)(name: String, tpt: Tree): TypeBind = copied(original, name, tpt)
    def unapply(x: TypeBind): (String, Tree) = (x.name, x.body)
  }

  object TypeBlock extends TypeBlockModule {
    def apply(aliases: List[TypeDef], tpt: TypeTree): TypeBlock = refused("TypeBlock.apply")
    def copy(original: Tree)(aliases: List[TypeDef], tpt: TypeTree): TypeBlock = copied(original, null, tpt, aliases)
    def unapply(x: TypeBlock): (List[TypeDef], TypeTree) = (x.aliases, x.tpt)
  }

  object TypeBoundsTree extends TypeBoundsTreeModule {
    def apply(low: TypeTree, hi: TypeTree): TypeBoundsTree = refused("TypeBoundsTree.apply")
    def copy(original: Tree)(low: TypeTree, hi: TypeTree): TypeBoundsTree = copied(original, null, low, hi)
    def unapply(x: TypeBoundsTree): (TypeTree, TypeTree) = (x.low, x.hi)
  }

  object WildcardTypeTree extends WildcardTypeTreeModule {
    def apply(tpe: TypeRepr): WildcardTypeTree = refused("WildcardTypeTree.apply")
    def unapply(x: WildcardTypeTree): true = true
  }

  object TypeCaseDef extends TypeCaseDefModule {
    def apply(pattern: TypeTree, rhs: TypeTree): TypeCaseDef = refused("TypeCaseDef.apply")
    def copy(original: Tree)(pattern: TypeTree, rhs: TypeTree): TypeCaseDef = copied(original, null, pattern, None, rhs)
    def unapply(tree: TypeCaseDef): (TypeTree, TypeTree) = (tree.pattern, tree.rhs)
  }

  abstract class TypeCaseDefMethodsImpl extends TypeCaseDefMethods {
    extension (self: TypeCaseDef) {
      def pattern: TypeTree = ReflectOps.part(self, 0)
      def rhs: TypeTree = ReflectOps.part(self, 2)
    }
  }

  abstract class MatchTypeTreeMethodsImpl extends MatchTypeTreeMethods {
    extension (self: MatchTypeTree) {
      def bound: Option[TypeTree] = throw new Unsupported("MatchTypeTree.bound")
      def selector: TypeTree = throw new Unsupported("MatchTypeTree.selector")
      def cases: List[TypeCaseDef] = throw new Unsupported("MatchTypeTree.cases")
    }
  }

  abstract class ByNameMethodsImpl extends ByNameMethods {
    extension (self: ByName) def result: TypeTree = ReflectOps.part(self, 0)
  }

  abstract class LambdaTypeTreeMethodsImpl extends LambdaTypeTreeMethods {
    extension (self: LambdaTypeTree) {
      def tparams: List[TypeDef] = ReflectOps.from(self, 1)
      def body: Tree = ReflectOps.part(self, 0)
    }
  }

  abstract class TypeBindMethodsImpl extends TypeBindMethods {
    extension (self: TypeBind) {
      def name: String = ReflectOps.shape(self).name
      def body: Tree = ReflectOps.part(self, 0)
    }
  }

  abstract class TypeBlockMethodsImpl extends TypeBlockMethods {
    extension (self: TypeBlock) {
      def aliases: List[TypeDef] = ReflectOps.from(self, 1)
      def tpt: TypeTree = ReflectOps.part(self, 0)
    }
  }

  abstract class TypeBoundsTreeMethodsImpl extends TypeBoundsTreeMethods {
    extension (self: TypeBoundsTree) {
      def tpe: TypeBounds = ReflectOps.tpe(self).asInstanceOf[TypeBounds]
      def low: TypeTree = ReflectOps.part(self, 0)
      def hi: TypeTree = ReflectOps.part(self, 1)
    }
  }

  abstract class WildcardTypeTreeMethodsImpl extends WildcardTypeTreeMethods {
    extension (self: WildcardTypeTree) def tpe: TypeRepr = ReflectOps.tpe(self).asInstanceOf[TypeRepr]
  }

  private def copied[T](original: Tree, name: String, parts: Any*): T = ReflectOps.copy(original, name, parts*).asInstanceOf[T]

  private def test[T](kinds: Int*): T = new KindTest(kinds*).asInstanceOf[T]

  private def refused(what: String): Nothing = throw new Unsupported(what)

  private def methods[T](name: String): T =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectTypeBoundSupport$ReflectTypeBounds$" + name).asInstanceOf[T]
}
}
