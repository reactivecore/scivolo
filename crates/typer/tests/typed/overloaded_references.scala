import scala.reflect.ClassTag

trait Test0
trait Test1[T]
trait Channel
trait MockSettings
trait A
trait B
abstract class C[E]

object Refs {
  def v: Test0 = ???
  def v[T]: Test1[Int] = ???
  val v1: Test0 = v

  def f(x: String): String = x
  def f[T >: Null](x: T): Int = 1
  val x4: String => String = f
  val x5: String => Any = f
  val x6: Any = f _

  def g(): Int = 1
  def g(x: Int): Int = 2
  val y1: Int => Int = g
  val y2: Any = g _

  def m(x: Float): Boolean = true
  def m(x: Float, y: Int): Boolean = false
  val y3: Function1[? >: Float, Boolean] = m _

  def mock[T: ClassTag]: T = ???
  def mock[T: ClassTag](settings: MockSettings): T = ???
  val case2: Channel => Any = mock

  def empty[E]: C[E] = ???
  def empty(implicit a: Any): Any = ???
  def statement(): Unit = empty

  def foo(f: B => Unit) = ???
  def transfer(in: A): Unit = foo(transfer)
  def transfer(in: B): Unit = ???
}
