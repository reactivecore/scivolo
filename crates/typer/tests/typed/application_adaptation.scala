trait Decoder[+T]:
  def apply(): T

given Decoder[Int] = new Decoder[Int]:
  def apply(): Int = 1

extension [A](a: A)
  def :*:[T](t: T): (A, T) = (a, t)

trait SuccOpt { type Refined <: Singleton }
class SuccClass extends SuccOpt { type Refined = this.type }

trait Numbers {
  type Nat
  type Succ <: Nat
  trait Opt { def refined: Succ }
}

object Nums extends Numbers {
  trait NatClass
  class Impl extends NatClass with Opt { def refined = this }
  type Nat = NatClass
  type Succ = Impl
}

trait Wrap { def wrap(n: Any): Option[n.type] }
class Wrapper extends Wrap { def wrap(n: Any) = Some(n) }

trait Got { type Out; def get: Out }

object Paths {
  def prepend[T <: Tuple](t: Got { type Out = T }): Int *: T = 1 *: t.get
  def bounded[T <: String](t: Got { type Out <: T }, u: t.Out): Int = 1
}

object TupleExt:
  extension [X <: Tuple](x: X)
    def indexOf(y: Any): Int = 0

object Sams {
  def cmp(a: Int, b: Int): Int = a - b
  def comp(c: java.util.Comparator[Int]): Int = c.compare(1, 2)
  def compared: Int = comp(cmp)
}

object Dirs {
  object home { def /(s: String) = dir }
  object dir {
    def mk(parents: Boolean = false)(using x: Int): Unit = ()
  }
  given Int = 3
  def made = (home / "temp").mk()
}

object Test {
  import TupleExt.*
  def index(names: Array[Object], x: Object): Int = names.indexOf(x)
  def int(n: BigInt): Int = n.intValue()
  def decoded: Int = summon[Decoder[Int]]()
  def pair = 1.:*:("one")
  def pair2 = 1 :*: "one"
  def refined(x: SuccClass): SuccOpt { type Refined <: x.type } = x
}
