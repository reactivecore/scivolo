//> using scala 3.9.0
def trace[A](a: A): A =
  println(s"arg $a")
  a

trait Named(val name: String):
  def hello = s"hello $name"

trait Counter(start: Int, val step: Int = 1):
  var count = start
  println(s"Counter.<init> $start")
  def next(): Int =
    count += step
    count

trait Logged(prefix: Any) extends Counter:
  val tag = s"[$prefix]"
  println(s"Logged.<init> $tag")

trait Boxed[T](x: T):
  def get: T = x

trait Ctx:
  def show(s: String): Unit

trait Uses(using c: Ctx):
  c.show("Uses")

trait Mid extends Uses

class Base(n: Int) extends Named(trace(s"base$n"))

class Sub extends Base(1) with Counter(trace(10), step = trace(2)) with Logged(trace("sub"))

class Rev extends Logged(trace("rev")) with Counter(trace(20))

class Gen extends Boxed[String]("boxed")

class WithCtx(using Ctx) extends Mid

trait Fn(val f: Int => Int)

class Lam(k: Int) extends Fn(x => x + k)

object Outer:
  trait In(val n: Int)
  class C extends In(3)

trait Clauses(a: Int)(val b: String):
  def a2 = a * 2

class Cl extends Clauses(21)("two")

trait VarParam(var v: Int):
  def bump() = v += 1

class Vp extends VarParam(4)

def local(k: Int) =
  trait LT(x: Int):
    private val z = 3
    def y = x + z + k
  class LC extends LT(k * 2)
  LC().y

@main def traitparams(): Unit =
  val s = Sub()
  println(s.hello)
  println(s.next())
  println(s.tag)
  val r = Rev()
  println(r.next())
  println(Gen().get)
  val a = new Named("anon") with Counter(5) {}
  println(a.hello + " " + a.next())
  WithCtx(using new Ctx { def show(s: String) = println(s"ctx $s") })
  println(Lam(2).f(1))
  println(Outer.C().n)
  val cl = Cl()
  println(s"${cl.a2} ${cl.b}")
  val vp = Vp()
  vp.bump()
  println(vp.v)
  println(local(5))
