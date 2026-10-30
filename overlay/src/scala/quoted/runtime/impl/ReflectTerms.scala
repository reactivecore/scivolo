// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.quoted.{Expr, Type}

/** The trees and terms of `quotes.reflect`. */
trait ReflectTermSupport { this: QuotesImpl =>

trait ReflectTerms extends reflectModule { self: reflect.type =>

  given TreeMethods: TreeMethods = methods("TreeMethodsImpl")
  given TermMethods: TermMethods = methods("TermMethodsImpl")
  given LiteralMethods: LiteralMethods = methods("LiteralMethodsImpl")

  object Term extends TermModule {
    def betaReduce(term: Term): Option[Term] =
      ReflectOps.opt(ReflectOps.one("betaReduce", ReflectOps.id(term))).asInstanceOf[Option[Term]]
  }

  object Ref extends RefModule {
    def term(tp: TermRef): Ref = throw new Unsupported("Ref.term")
    def apply(sym: Symbol): Ref = termOf(ReflectOps.one("ref", ReflectOps.id(sym))).asInstanceOf[Ref]
  }

  object Literal extends LiteralModule {
    def apply(constant: Constant): Literal = {
      val c = constant.asInstanceOf[ConstantImpl]
      ReflectOps.run("literal", Array(c.kind, c.lo, c.hi), c.text)
      termOf(QuoteHost.result(0)).asInstanceOf[Literal]
    }
    def copy(original: Tree)(constant: Constant): Literal = throw new Unsupported("Literal.copy")
    def unapply(x: Literal): Some[Constant] = Some(x.constant)
  }

  given IdentMethods: IdentMethods = methods("IdentMethodsImpl")
  given SelectMethods: SelectMethods = methods("SelectMethodsImpl")
  given TypedMethods: TypedMethods = methods("TypedMethodsImpl")
  given ApplyMethods: ApplyMethods = methods("ApplyMethodsImpl")
  given TypeApplyMethods: TypeApplyMethods = methods("TypeApplyMethodsImpl")
  given BlockMethods: BlockMethods = methods("BlockMethodsImpl")
  given InlinedMethods: InlinedMethods = methods("InlinedMethodsImpl")
  given RepeatedMethods: RepeatedMethods = methods("RepeatedMethodsImpl")
  given TypeTreeMethods: TypeTreeMethods = methods("TypeTreeMethodsImpl")
  given ValDefMethods: ValDefMethods = methods("ValDefMethodsImpl")
  given NamedArgMethods: NamedArgMethods = methods("NamedArgMethodsImpl")
  given DefinitionMethods: DefinitionMethods = methods("DefinitionMethodsImpl")

  abstract class TreeMethodsImpl extends TreeMethods {
    extension (self: Tree) {
      def asExpr: Expr[Any] = QuoteHost.expr(ReflectOps.id(self))
      def asExprOf[T](using t: Type[T]): Expr[T] = {
        if (ReflectOps.one("isExprOf", ReflectOps.id(self), QuoteHost.typeId(t)) == 0)
          throw new Unsupported("Tree.asExprOf of a term of another type")
        self.asExpr.asInstanceOf[Expr[T]]
      }
      def pos: Position = ReflectOps.position("pos", ReflectOps.id(self)).asInstanceOf[Position]
      def show(using printer: Printer[Tree]): String = printer.show(self)
      def symbol: Symbol = new SymbolImpl(ReflectOps.one("treeSymbol", ReflectOps.id(self))).asInstanceOf[Symbol]
      def isExpr: Boolean = ReflectOps.isExpr(self)
    }
    extension [ThisTree <: Tree](self: ThisTree) def changeOwner(newOwner: Symbol): ThisTree =
      ReflectOps.term(ReflectOps.one("changeOwner", ReflectOps.id(self), ReflectOps.id(newOwner))).asInstanceOf[ThisTree]
  }

  abstract class IdentMethodsImpl extends IdentMethods {
    extension (self: Ident) def name: String = ReflectOps.shape(self).name
  }

  abstract class SelectMethodsImpl extends SelectMethods {
    extension (self: Select) {
      def qualifier: Term = ReflectOps.shape(self).child(0).asInstanceOf[Term]
      def name: String = ReflectOps.shape(self).name
    }
  }

  abstract class TypedMethodsImpl extends TypedMethods {
    extension (self: Typed) {
      def expr: Term = ReflectOps.shape(self).child(0).asInstanceOf[Term]
      def tpt: TypeTree = ReflectOps.shape(self).child(1).asInstanceOf[TypeTree]
    }
  }

  abstract class ApplyMethodsImpl extends ApplyMethods {
    extension (self: Apply) {
      def fun: Term = ReflectOps.shape(self).child(0).asInstanceOf[Term]
      def args: List[Term] = ReflectOps.shape(self).from(1).asInstanceOf[List[Term]]
    }
  }

  abstract class TypeApplyMethodsImpl extends TypeApplyMethods {
    extension (self: TypeApply) {
      def fun: Term = ReflectOps.shape(self).child(0).asInstanceOf[Term]
      def args: List[TypeTree] = ReflectOps.shape(self).from(1).asInstanceOf[List[TypeTree]]
    }
  }

  abstract class BlockMethodsImpl extends BlockMethods {
    extension (self: Block) {
      def statements: List[Statement] = ReflectOps.shape(self).from(1).asInstanceOf[List[Statement]]
      def expr: Term = ReflectOps.shape(self).child(0).asInstanceOf[Term]
    }
  }

  abstract class InlinedMethodsImpl extends InlinedMethods {
    extension (self: Inlined) {
      def call: Option[Tree] = ReflectOps.opt(ReflectOps.shape(self).children(0)).asInstanceOf[Option[Tree]]
      def bindings: List[Definition] = ReflectOps.shape(self).from(2).asInstanceOf[List[Definition]]
      def body: Term = ReflectOps.shape(self).child(1).asInstanceOf[Term]
    }
  }

  abstract class RepeatedMethodsImpl extends RepeatedMethods {
    extension (self: Repeated) {
      def elems: List[Term] = ReflectOps.shape(self).from(1).asInstanceOf[List[Term]]
      def elemtpt: TypeTree = ReflectOps.shape(self).child(0).asInstanceOf[TypeTree]
    }
  }

  abstract class TypeTreeMethodsImpl extends TypeTreeMethods {
    extension (self: TypeTree) def tpe: TypeRepr = QuoteHost.tpe(ReflectOps.one("tpe", ReflectOps.id(self))).asInstanceOf[TypeRepr]
  }

  abstract class ValDefMethodsImpl extends ValDefMethods {
    extension (self: ValDef) {
      def tpt: TypeTree = ReflectOps.shape(self).child(0).asInstanceOf[TypeTree]
      def rhs: Option[Term] = ReflectOps.opt(ReflectOps.shape(self).children(1)).asInstanceOf[Option[Term]]
    }
  }

  abstract class NamedArgMethodsImpl extends NamedArgMethods {
    extension (self: NamedArg) {
      def name: String = ReflectOps.shape(self).name
      def value: Term = ReflectOps.shape(self).child(0).asInstanceOf[Term]
    }
  }

  abstract class DefinitionMethodsImpl extends DefinitionMethods {
    extension (self: Definition) def name: String = ReflectOps.shape(self).name
  }

  abstract class TermMethodsImpl extends TermMethods {
    extension (self: Term) {
      def tpe: TypeRepr = QuoteHost.tpe(ReflectOps.one("tpe", ReflectOps.id(self))).asInstanceOf[TypeRepr]
      def underlyingArgument: Term =
        ReflectOps.term(ReflectOps.one("underlyingArgument", ReflectOps.id(self))).asInstanceOf[Term]
      def underlying: Term = ReflectOps.term(ReflectOps.one("underlying", ReflectOps.id(self))).asInstanceOf[Term]
      def select(sym: Symbol): Select =
        ReflectOps.term(ReflectOps.one("select", ReflectOps.id(self), ReflectOps.id(sym))).asInstanceOf[Select]
      def etaExpand(owner: Symbol): Term =
        ReflectOps.term(ReflectOps.one("etaExpand", ReflectOps.id(self), ReflectOps.id(owner))).asInstanceOf[Term]
      def appliedTo(arg: Term): Term = self.appliedToArgs(List(arg))
      def appliedTo(arg: Term, args: Term*): Term = self.appliedToArgs(arg :: args.toList)
      def appliedToArgs(args: List[Term]): Apply =
        ReflectOps.term(ReflectOps.one("apply", (self :: args).map(ReflectOps.id)*)).asInstanceOf[Apply]
      def appliedToNone: Apply = self.appliedToArgs(Nil)
      def appliedToArgss(argss: List[List[Term]]): Term = argss.foldLeft(self)(_.appliedToArgs(_))
      def ensureApplied: Term = if (ReflectOps.widenedKind(self) == 8) self.appliedToNone else self
      def appliedToTypeTrees(targs: List[TypeTree]): Term =
        if (targs.isEmpty) self else self.appliedToTypes(targs.map(ReflectOps.tpe(_).asInstanceOf[TypeRepr]))
      def seal: Expr[Any] =
        if (ReflectOps.isExpr(self)) self.asExpr else throw new Unsupported("Term.seal of a partially applied term")
      def sealOpt: Option[Expr[Any]] = if (ReflectOps.isExpr(self)) Some(self.asExpr) else None
      def appliedToType(targ: TypeRepr): Term = self.appliedToTypes(List(targ))
      def appliedToTypes(targs: List[TypeRepr]): Term = {
        val args = ReflectOps.id(self) :: targs.map(ReflectOps.id)
        ReflectOps.term(ReflectOps.one("appliedToTypes", args*)).asInstanceOf[Term]
      }
    }
  }

  abstract class LiteralMethodsImpl extends LiteralMethods {
    extension (self: Literal) def constant: Constant = {
      ReflectOps.run("constant", Array(ReflectOps.id(self)), null)
      new ConstantImpl(QuoteHost.result(0), QuoteHost.result(1), QuoteHost.result(2), QuoteHost.resultText())
        .asInstanceOf[Constant]
    }
  }

  private def methods[T](name: String): T =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectTermSupport$ReflectTerms$" + name).asInstanceOf[T]

  private def termOf(id: Int): Term = ReflectOps.term(id).asInstanceOf[Term]
}
}
