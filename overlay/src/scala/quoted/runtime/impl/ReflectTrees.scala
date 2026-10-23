// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

import scala.quoted.Type
import scala.reflect.TypeTest

/** The type tests and extractors of the trees of `quotes.reflect`, over the frontend's tree
 *  kinds (`ReflectOps.kind`).
 */
trait ReflectTreeSupport { this: QuotesImpl =>

trait ReflectTrees extends reflectModule { self: reflect.type =>

  given TermTypeTest: TypeTest[Tree, Term] = test(1, 2, 3, 4, 5, 6, 7, 8, 9, 13, 14, 18)
  given StatementTypeTest: TypeTest[Tree, Statement] = test(1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16, 17, 18, 19)
  given ThisTypeTest: TypeTest[Tree, This] = test(18)

  object This extends ThisModule {
    def apply(cls: Symbol): This = refused("This.apply")
    def copy(original: Tree)(qual: Option[String]): This = refused("This.copy")
    def unapply(x: This): Some[Option[String]] = Some(Option(ReflectOps.shape(x).name))
  }
  given DefinitionTypeTest: TypeTest[Tree, Definition] = test(12, 15, 16, 19)
  given IdentTypeTest: TypeTest[Tree, Ident] = test(1)
  given SelectTypeTest: TypeTest[Tree, Select] = test(2)
  given LiteralTypeTest: TypeTest[Tree, Literal] = test(3)
  given TypedTypeTest: TypeTest[Tree, Typed] = test(4)
  given ApplyTypeTest: TypeTest[Tree, Apply] = test(5)
  given TypeApplyTypeTest: TypeTest[Tree, TypeApply] = test(6)
  given BlockTypeTest: TypeTest[Tree, Block] = test(7)
  given InlinedTypeTest: TypeTest[Tree, Inlined] = test(8)
  given RepeatedTypeTest: TypeTest[Tree, Repeated] = test(9)
  given TypeTreeTypeTest: TypeTest[Tree, TypeTree] = test(10, 11)
  given InferredTypeTest: TypeTest[Tree, Inferred] = test(11)
  given ValDefTypeTest: TypeTest[Tree, ValDef] = test(12)
  given NamedArgTypeTest: TypeTest[Tree, NamedArg] = test(13)

  object Ident extends IdentModule {
    def apply(tmref: TermRef): Term = ReflectOps.term(ReflectOps.one("identRef", ReflectOps.id(tmref))).asInstanceOf[Term]
    def copy(original: Tree)(name: String): Ident = refused("Ident.copy")
    def unapply(tree: Ident): Some[String] = Some(ReflectOps.shape(tree).name)
  }

  object Select extends SelectModule {
    def apply(qualifier: Term, symbol: Symbol): Select = qualifier.select(symbol)
    def unique(qualifier: Term, name: String): Select = refused("Select.unique")
    def overloaded(qualifier: Term, name: String, targs: List[TypeRepr], args: List[Term]): Term =
      refused("Select.overloaded")
    def overloaded(qualifier: Term, name: String, targs: List[TypeRepr], args: List[Term], returnType: TypeRepr): Term =
      refused("Select.overloaded")
    def copy(original: Tree)(qualifier: Term, name: String): Select = refused("Select.copy")
    def unapply(x: Select): (Term, String) = {
      val s = ReflectOps.shape(x)
      (s.child(0).asInstanceOf[Term], s.name)
    }
  }

  object Typed extends TypedModule {
    def apply(expr: Term, tpt: TypeTree): Typed = built("typed", List(expr, tpt)).asInstanceOf[Typed]
    def copy(original: Tree)(expr: Term, tpt: TypeTree): Typed = refused("Typed.copy")
    def unapply(x: Typed): (Term, TypeTree) = {
      val s = ReflectOps.shape(x)
      (s.child(0).asInstanceOf[Term], s.child(1).asInstanceOf[TypeTree])
    }
  }

  object Apply extends ApplyModule {
    def apply(fun: Term, args: List[Term]): Apply = built("apply", fun :: args).asInstanceOf[Apply]
    def copy(original: Tree)(fun: Term, args: List[Term]): Apply = refused("Apply.copy")
    def unapply(x: Apply): (Term, List[Term]) = {
      val s = ReflectOps.shape(x)
      (s.child(0).asInstanceOf[Term], s.from(1).asInstanceOf[List[Term]])
    }
  }

  object TypeApply extends TypeApplyModule {
    def apply(fun: Term, args: List[TypeTree]): TypeApply = fun.appliedToTypes(args.map(_.tpe)).asInstanceOf[TypeApply]
    def copy(original: Tree)(fun: Term, args: List[TypeTree]): TypeApply = refused("TypeApply.copy")
    def unapply(x: TypeApply): (Term, List[TypeTree]) = {
      val s = ReflectOps.shape(x)
      (s.child(0).asInstanceOf[Term], s.from(1).asInstanceOf[List[TypeTree]])
    }
  }

  object Block extends BlockModule {
    def apply(stats: List[Statement], expr: Term): Block = built("block", expr :: stats).asInstanceOf[Block]
    def copy(original: Tree)(stats: List[Statement], expr: Term): Block = refused("Block.copy")
    def unapply(x: Block): (List[Statement], Term) = {
      val s = ReflectOps.shape(x)
      (s.from(1).asInstanceOf[List[Statement]], s.child(0).asInstanceOf[Term])
    }
  }

  object Inlined extends InlinedModule {
    def apply(call: Option[Tree], bindings: List[Definition], expansion: Term): Inlined = refused("Inlined.apply")
    def copy(original: Tree)(call: Option[Tree], bindings: List[Definition], expansion: Term): Inlined =
      refused("Inlined.copy")
    def unapply(x: Inlined): (Option[Tree], List[Definition], Term) = {
      val s = ReflectOps.shape(x)
      (ReflectOps.opt(s.children(0)).asInstanceOf[Option[Tree]], s.from(2).asInstanceOf[List[Definition]],
        s.child(1).asInstanceOf[Term])
    }
  }

  object Repeated extends RepeatedModule {
    def apply(elems: List[Term], tpt: TypeTree): Repeated =
      ReflectOps.term(ReflectOps.one("repeated", (elems :+ tpt).map(ReflectOps.id)*)).asInstanceOf[Repeated]
    def copy(original: Tree)(elems: List[Term], tpt: TypeTree): Repeated = refused("Repeated.copy")
    def unapply(x: Repeated): (List[Term], TypeTree) = {
      val s = ReflectOps.shape(x)
      (s.from(1).asInstanceOf[List[Term]], s.child(0).asInstanceOf[TypeTree])
    }
  }

  object TypeTree extends TypeTreeModule {
    def of[T <: AnyKind](using t: Type[T]): TypeTree =
      ReflectOps.term(ReflectOps.one("typeTree", QuoteHost.typeId(t))).asInstanceOf[TypeTree]
    def ref(typeSymbol: Symbol): TypeTree = refused("TypeTree.ref")
  }

  object Inferred extends InferredModule {
    def apply(tpe: TypeRepr): Inferred = ReflectOps.term(ReflectOps.one("typeTree", ReflectOps.id(tpe))).asInstanceOf[Inferred]
    def unapply(x: Inferred): true = true
  }

  object ValDef extends ValDefModule {
    def apply(symbol: Symbol, rhs: Option[Term]): ValDef = refused("ValDef.apply")
    def copy(original: Tree)(name: String, tpt: TypeTree, rhs: Option[Term]): ValDef = refused("ValDef.copy")
    def unapply(vdef: ValDef): (String, TypeTree, Option[Term]) = {
      val s = ReflectOps.shape(vdef)
      (s.name, s.child(0).asInstanceOf[TypeTree], ReflectOps.opt(s.children(1)).asInstanceOf[Option[Term]])
    }
    def let(owner: Symbol, name: String, rhs: Term, flags: Flags)(body: Ref => Term): Term = refused("ValDef.let")
    def let(owner: Symbol, name: String, rhs: Term)(body: Ref => Term): Term = refused("ValDef.let")
    def let(owner: Symbol, terms: List[Term])(body: List[Ref] => Term): Term = refused("ValDef.let")
  }

  object NamedArg extends NamedArgModule {
    def apply(name: String, arg: Term): NamedArg = refused("NamedArg.apply")
    def copy(original: Tree)(name: String, arg: Term): NamedArg = refused("NamedArg.copy")
    def unapply(x: NamedArg): (String, Term) = {
      val s = ReflectOps.shape(x)
      (s.name, s.child(0).asInstanceOf[Term])
    }
  }

  object Match extends MatchModule {
    def apply(selector: Term, cases: List[CaseDef]): Match = built("match", selector :: cases).asInstanceOf[Match]
    def copy(original: Tree)(selector: Term, cases: List[CaseDef]): Match = refused("Match.copy")
    def unapply(x: Match): (Term, List[CaseDef]) = refused("Match.unapply")
  }

  object CaseDef extends CaseDefModule {
    def apply(pattern: Tree, guard: Option[Term], rhs: Term): CaseDef = {
      val args = Array(ReflectOps.id(pattern), guard.fold(-1)(ReflectOps.id), ReflectOps.id(rhs))
      ReflectOps.term(ReflectOps.one("caseDef", args*)).asInstanceOf[CaseDef]
    }
    def copy(original: Tree)(pattern: Tree, guard: Option[Term], rhs: Term): CaseDef = refused("CaseDef.copy")
    def unapply(x: CaseDef): (Tree, Option[Term], Term) = refused("CaseDef.unapply")
  }

  object Wildcard extends WildcardModule {
    def apply(): Wildcard = ReflectOps.term(ReflectOps.one("wildcard")).asInstanceOf[Wildcard]
    def unapply(pattern: Wildcard): true = true
  }

  private def built(op: String, trees: List[Tree]): Any = ReflectOps.term(ReflectOps.one(op, trees.map(ReflectOps.id)*))

  private def test[T](kinds: Int*): T = new KindTest(kinds*).asInstanceOf[T]

  private def refused(what: String): Nothing = throw new Unsupported(what)
}
}
