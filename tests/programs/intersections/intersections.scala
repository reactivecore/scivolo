//> using scala 3.9.0
class A:
  private def foo = 1
  def viaAB(other: A & B): Int = other.foo
  def viaBA(other: B & A): Int = other.foo

trait B:
  def foo(i: Int): Int = i * 10

trait HasT:
  type T = Int
  def mk: T = 3

trait Other

trait Base:
  def name: String = "base"

trait Left extends Base:
  override def name: String = "left"

trait Right extends Base

trait Shown:
  def show: String = "shown"

trait Hidden:
  private def show: String = "hidden"

trait JX:
  override def toString = "jx"
trait JY
class JXY extends JX with JY
trait JA:
  def f: JX
trait JB:
  def f: JY
class JAB extends JA with JB:
  def f: JXY = new JXY

trait Tc[T]:
  def name: String
given Tc[JX] with
  def name = "TcX"
given Tc[JY] with
  def name = "TcY"
given Tc[JX & JY] with
  def name = "TcXY"
def tc[T](t: T)(using c: Tc[T]): String = c.name

trait SelfJoint:
  self: JA & JB =>
  def use: String = "" + f

def right(x: Other & HasT): Int =
  val t: x.T = x.mk
  t
def named(x: Right & Left): String = x.name
def shown(x: Hidden & Shown): String = x.show

@main def intersections(): Unit =
  val a = new A with B
  println(a.viaAB(a))
  println(a.viaBA(a))
  println(a.foo(4))
  println(right(new Other with HasT {}))
  println(named(new Right with Left {}))
  println(shown(new Hidden with Shown {}))
  val ab: JA & JB = new JAB
  println(tc(ab.f))
  println((new JAB with SelfJoint).use)
