//> using scala 3.9.0
trait Effect
abstract class Fun[-X, +Y]:
  type Eff <: Effect
  def apply(x: X): Eff ?=> Y

class Throws extends Effect

trait Box:
  type T
  def value: T

object Funs:
  val show = new Fun[Int, String] { type Eff = Throws; def apply(x: Int) = "#" + x }
  given Throws = new Throws

  def map[A, B](f: Fun[A, B])(xs: List[A]): f.Eff ?=> List[B] = xs.map(f.apply)

  val mapFn: (f: Fun[Int, String]) => List[Int] => f.Eff ?=> List[String] =
    f => xs => map(f)(xs)

  val unbox: (b: Box) => b.T = b => b.value
  val plain: (x: Int) => Int = x => x + 1
  val first: (b: Box) ?=> b.T = summon[Box].value

  def boxed(n: Int): Box { type T = Int } = new Box { type T = Int; def value = n }

@main def ty24dependent(): Unit =
  import Funs.{*, given}
  println(map(show)(List(1, 2, 3)))
  println(mapFn(show)(List(4, 5)))
  val b = boxed(7)
  val n: Int = unbox(b)
  println(n + plain(1))
  println(first(using b))
  println(List(boxed(1), boxed(2)).map(x => x.value).sum)
