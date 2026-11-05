//> using scala 3.9.0
trait Ctx:
  def name: String

object Registry:
  object fallback extends Ctx:
    def name = "fallback"
  val global: Ctx = Holder("global")

class Holder(val n: String) extends Ctx:
  val other: Ctx = Registry.global
  def name: String = n
  given self: Ctx = this
  given sibling: Ctx = other
  given selected: Ctx = this.other
  given fresh: Ctx = Holder(n + "'")
  given outside: Ctx = Registry.global
  def show(using c: Ctx): String = c.name
  def local: String =
    given Ctx = Holder.this
    show
  def param(c: Ctx): String =
    given Ctx = c
    show

  class Inner:
    given outer: Ctx = Holder.this
    given outerMember: Ctx = sibling

abstract class Compound:
  def first: Int = a
  protected lazy val (a, (b, c: String), d) = (1, (2, "three"), 4.0)
  def all: String = s"$a $b $c $d"

class Impl extends Compound

class Sequenced:
  given before: Sequenced = after
  given after: Sequenced = this
  given chained: Sequenced = after
  inline def myself: this.type = this
  given inlined: Sequenced = myself
  var single: Registry.fallback.type = Registry.fallback
  given ofVar: Ctx = single

class Imported extends Holder("i"):
  import Registry.global as other2
  given viaImport: Ctx = other2

object Statics:
  val r: Registry.type = Registry
  lazy val viaVal = r.fallback
  lazy val obj: Registry.fallback.type = Registry.fallback
  lazy val any: Any = Registry.fallback
  lazy val inferred = Registry.fallback
  given canEqualCtx: CanEqual[Ctx, Ctx] = CanEqual.derived
  given me: Statics.type = this

@main def givenaliases(): Unit =
  val h = Holder("h")
  println(h.local)
  println(h.param(Registry.fallback))
  println(List(h.self, h.sibling, h.selected, h.fresh, h.outside).map(_.name).mkString(","))
  println(h.fresh eq h.fresh)
  val in = h.Inner()
  println(in.outer.name + in.outerMember.name)
  val i = Impl()
  println(i.first)
  println(i.all)
  println(Statics.obj.name + (Statics.any == Registry.fallback) + Statics.inferred.name)
  println(Statics.canEqualCtx eq Statics.canEqualCtx)
  println(Statics.me eq Statics)
  val o = Sequenced()
  println(List(o.before, o.after, o.chained, o.inlined).forall(_ eq o) && (o.ofVar eq Registry.fallback))
  println(Imported().viaImport.name + Statics.viaVal.name)
