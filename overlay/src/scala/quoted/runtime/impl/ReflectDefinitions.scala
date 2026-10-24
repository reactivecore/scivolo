// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.reflect.TypeTest

/** The class and method definitions of `quotes.reflect` and the annotations and trees of
 *  symbols.
 */
trait ReflectDefinitionSupport { this: QuotesImpl =>

trait ReflectDefinitions extends reflectModule { self: reflect.type =>

  given ClassDefTypeTest: TypeTest[Tree, ClassDef] = kind(19)
  given DefDefTypeTest: TypeTest[Tree, DefDef] = kind(15)

  object ClassDef extends ClassDefModule {
    def apply(cls: Symbol, parents: List[Tree], body: List[Statement]): ClassDef = refused("ClassDef.apply")
    def copy(original: Tree)(name: String, constr: DefDef, parents: List[Tree], selfOpt: Option[ValDef],
        body: List[Statement]): ClassDef = refused("ClassDef.copy")
    def unapply(cdef: ClassDef): (String, DefDef, List[Tree], Option[ValDef], List[Statement]) =
      refused("ClassDef.unapply")
    def module(module: Symbol, parents: List[Tree], body: List[Statement]): (ValDef, ClassDef) =
      refused("ClassDef.module")
  }

  object DefDef extends DefDefModule {
    def apply(symbol: Symbol, rhsFn: List[List[Tree]] => Option[Term]): DefDef = refused("DefDef.apply")
    def copy(original: Tree)(name: String, paramss: List[ParamClause], tpt: TypeTree, rhs: Option[Term]): DefDef =
      ReflectOps.copyDef(original, name, paramss.flatMap(_.asInstanceOf[ClauseImpl].params), tpt, rhs).asInstanceOf[DefDef]
    def unapply(ddef: DefDef): (String, List[ParamClause], TypeTree, Option[Term]) = {
      val s = ReflectOps.shape(ddef)
      (s.name, ddef.paramss, s.child(0).asInstanceOf[TypeTree], ReflectOps.opt(s.children(1)).asInstanceOf[Option[Term]])
    }
  }

  given ClassDefMethods: ClassDefMethods = methods("ClassDefMethodsImpl")
  given DefDefMethods: DefDefMethods = methods("DefDefMethodsImpl")
  given ParamClauseMethods: ParamClauseMethods = methods("ParamClauseMethodsImpl")
  given TermParamClauseMethods: TermParamClauseMethods = methods("TermParamClauseMethodsImpl")
  given TermParamClauseTypeTest: TypeTest[ParamClause, TermParamClause] = clauses(false)
  given TypeParamClauseTypeTest: TypeTest[ParamClause, TypeParamClause] = clauses(true)

  abstract class ClassDefMethodsImpl extends ClassDefMethods {
    extension (self: ClassDef) def body: List[Statement] =
      ReflectOps.results("classBody", ReflectOps.id(self)).map(ReflectOps.term).asInstanceOf[List[Statement]]
  }

  abstract class DefDefMethodsImpl extends DefDefMethods {
    extension (self: DefDef) {
      def paramss: List[ParamClause] = {
        val r = ReflectOps.results("defClauses", ReflectOps.id(self))
        val n = r.head
        val shapes = r.slice(1, 1 + 2 * n).grouped(2).toList
        val params = r.drop(1 + 2 * n).map(ReflectOps.term)
        val starts = shapes.scanLeft(0)(_ + _(1))
        shapes.zip(starts).map((c, start) => new ClauseImpl(c(0) != 0, params.slice(start, start + c(1))))
          .asInstanceOf[List[ParamClause]]
      }
      def returnTpt: TypeTree = ReflectOps.shape(self).child(0).asInstanceOf[TypeTree]
      def rhs: Option[Term] = ReflectOps.opt(ReflectOps.shape(self).children(1)).asInstanceOf[Option[Term]]
    }
  }

  abstract class ParamClauseMethodsImpl extends ParamClauseMethods {
    extension (self: ParamClause) def params: List[ValDef] | List[TypeDef] =
      self.asInstanceOf[ClauseImpl].params.asInstanceOf[List[ValDef]]
  }

  abstract class TermParamClauseMethodsImpl extends TermParamClauseMethods {
    extension (self: TermParamClause) def params: List[ValDef] =
      self.asInstanceOf[ClauseImpl].params.asInstanceOf[List[ValDef]]
  }

  private def clauses[T](isType: Boolean): TypeTest[ParamClause, T] = new TypeTest[ParamClause, T] {
    def unapply(x: ParamClause): Option[x.type & T] =
      if (x.asInstanceOf[ClauseImpl].isType == isType) Some(x.asInstanceOf[x.type & T]) else None
  }

  private def kind[T](k: Int): T = new KindTest(k).asInstanceOf[T]

  private def refused(what: String): Nothing = throw new Unsupported(what)

  private def methods[T](name: String): T =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectDefinitionSupport$ReflectDefinitions$" + name).asInstanceOf[T]
}
}

/** A clause of type parameters if `isType`, else of term parameters. */
final class ClauseImpl(val isType: Boolean, val params: List[Any])
