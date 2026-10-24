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
    def apply(cls: Symbol, parents: List[Tree], body: List[Statement]): ClassDef = {
      val ids = List(ReflectOps.id(cls), parents.length) ::: (parents ::: body).map(ReflectOps.id)
      ReflectOps.term(ReflectOps.one("classDef", ids*)).asInstanceOf[ClassDef]
    }
    def copy(original: Tree)(name: String, constr: DefDef, parents: List[Tree], selfOpt: Option[ValDef],
        body: List[Statement]): ClassDef = {
      val ids = List(ReflectOps.id(original), ReflectOps.id(constr), selfOpt.fold(-1)(ReflectOps.id), parents.length) :::
        (parents ::: body).map(ReflectOps.id)
      ReflectOps.run("copyClass", ids.toArray, name)
      ReflectOps.term(QuoteHost.result(0)).asInstanceOf[ClassDef]
    }
    def unapply(cdef: ClassDef): (String, DefDef, List[Tree], Option[ValDef], List[Statement]) = {
      val r = ReflectOps.results("classParts", ReflectOps.id(cdef))
      val name = QuoteHost.resultText()
      val (parents, body) = r.drop(3).map(ReflectOps.term).splitAt(r(2))
      (name, ReflectOps.term(r(0)).asInstanceOf[DefDef], parents.asInstanceOf[List[Tree]],
        ReflectOps.opt(r(1)).asInstanceOf[Option[ValDef]], body.asInstanceOf[List[Statement]])
    }
    def module(module: Symbol, parents: List[Tree], body: List[Statement]): (ValDef, ClassDef) =
      refused("ClassDef.module")
  }

  object DefDef extends DefDefModule {
    def apply(symbol: Symbol, rhsFn: List[List[Tree]] => Option[Term]): DefDef = {
      val r = ReflectOps.results("defParams", ReflectOps.id(symbol))
      val sizes = r.slice(1, 1 + r.head)
      val refs = r.drop(1 + r.head).map(ReflectOps.term).asInstanceOf[List[Tree]]
      val paramss = sizes.scanLeft(0)(_ + _).zip(sizes).map((start, n) => refs.slice(start, start + n))
      val rhs = rhsFn(paramss)
      ReflectOps.term(ReflectOps.one("defDef", ReflectOps.id(symbol), rhs.fold(-1)(ReflectOps.id))).asInstanceOf[DefDef]
    }
    def copy(original: Tree)(name: String, paramss: List[ParamClause], tpt: TypeTree, rhs: Option[Term]): DefDef =
      ReflectOps.copyDef(original, name, paramss.flatMap(_.asInstanceOf[ClauseImpl].params), tpt, rhs).asInstanceOf[DefDef]
    def unapply(ddef: DefDef): (String, List[ParamClause], TypeTree, Option[Term]) = {
      val s = ReflectOps.shape(ddef)
      (s.name, ddef.paramss, s.child(0).asInstanceOf[TypeTree], ReflectOps.opt(s.children(1)).asInstanceOf[Option[Term]])
    }
  }

  object New extends NewModule {
    def apply(tpt: TypeTree): New = ReflectOps.term(ReflectOps.one("new", ReflectOps.id(tpt))).asInstanceOf[New]
    def copy(original: Tree)(tpt: TypeTree): New = refused("New.copy")
    def unapply(x: New): Some[TypeTree] = refused("New.unapply")
  }

  object TypeIdent extends TypeIdentModule {
    def apply(sym: Symbol): TypeTree = ReflectOps.term(ReflectOps.one("typeIdent", ReflectOps.id(sym))).asInstanceOf[TypeTree]
    def copy(original: Tree)(name: String): TypeIdent = refused("TypeIdent.copy")
    def unapply(x: TypeIdent): Some[String] = refused("TypeIdent.unapply")
  }

  object TermParamClause extends TermParamClauseModule {
    def apply(params: List[ValDef]): TermParamClause = new ClauseImpl(false, params).asInstanceOf[TermParamClause]
    def unapply(x: TermParamClause): Some[List[ValDef]] = Some(x.params)
  }

  object TypeParamClause extends TypeParamClauseModule {
    def apply(params: List[TypeDef]): TypeParamClause = new ClauseImpl(true, params).asInstanceOf[TypeParamClause]
    def unapply(x: TypeParamClause): Some[List[TypeDef]] =
      Some(x.asInstanceOf[ClauseImpl].params.asInstanceOf[List[TypeDef]])
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
