import language.experimental.captureChecking

class Label[T]
class A
class B

trait Base:
  def l: Label[Int]^{}
  def m: Label[Int]^{}

object Generic:
  def id[T](x: T): T = x
  def g(a: A^, b: B^{a}): Int =
    val y = id(b)
    val z = List(b)
    val s = s"of $b"
    1

object Conds:
  def g(a: A^, b: B^{a}, c: Boolean): Int =
    val y = if c then b else b
    val z = c match
      case true => b
      case false => b
    1

object Overrides extends Base:
  val l = Label[Int]()
  def m = l

object Closures:
  def g(a: A^, b: B^{a}): Int =
    val f = (x: Int) => b
    val h = () => b
    1

class Queue[E]
object Queue:
  def from[E](it: IterableOnce[E]^): Queue[E] = Queue[E]()

object Outer:
  def g[E](it: IterableOnce[E]^): Queue[E] = Queue.from(it)
