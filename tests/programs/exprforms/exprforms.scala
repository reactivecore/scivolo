//> using scala 3.9.0
trait TA
trait TB
class X
class Y
class E(val code: Int) extends Exception
case class A(x: String) extends Throwable
class B extends Exception { override def toString = "B" }

trait Typed { val actualType: Class[?] }
trait Bools extends Typed { final val actualType = classOf[Boolean] }

object Classes extends Bools:
  class C
  final val k = classOf[C]
  def run(): Unit =
    val z: Class[C] = classOf
    println(classOf[C].getName)
    println(z eq k)
    println(actualType.getName)
    println(List[Class[?]](classOf[Long], classOf[Array[Int]], classOf[String]))
    val c: Any = if z eq null then classOf[Unit] else classOf[Array[String]]
    println(c)

object Imports:
  def larger(): Int =
    import scala.math.max
    max(3, 4)
  def run(): Unit =
    import scala.collection.mutable.ListBuffer
    println(ListBuffer(1, 2))
    println(larger())
    val f = (i: Int) => { import scala.math.min; min(i, 2) }
    println(f(5))

object Catches:
  def f(i: Int): Int = if i > 0 then throw new E(i) else i
  def guarded(i: Int): String =
    try { f(i); "none" }
    catch
      case e: IllegalStateException => "ise"
      case e: E if e.code > 5 => "big " + e.code
      case e: E => "small " + e.code
  def nested(i: Int): String =
    try
      try f(i).toString
      catch case e: E if e.code > 2 => "inner"
    catch
      case e: E if e.code > 1 => "outer"
      case _: RuntimeException => "rt"
  def patterns(t: Throwable): String =
    try throw t
    catch
      case A(s) => s
      case _: TA => "ta"
      case _: B | _: IllegalStateException => "b"
      case e if e.getMessage == null => "null"
  def run(): Unit =
    println(guarded(0))
    println(guarded(3))
    println(guarded(7))
    println(nested(3))
    println(nested(2))
    println(patterns(A("a")))
    println(patterns(new Exception with TA))
    println(patterns(new B))
    println(patterns(new Exception))

class Nothings:
  var z = 0
  def assigned(x: Boolean): Int =
    var y = 0
    if x then y = throw new Error("y")
    else y = 1
    y
  def field(): Unit = z = throw new Error("z")
  def argument(): Unit = g(throw new Error("g"))
  def g(i: Int): Unit = ()

object Casts:
  def anyInt(x: Any): Int = x.asInstanceOf[Int]
  def run(): Unit =
    val a = { 5 }.asInstanceOf[Unit]
    val b = { println("hello!"); 5 }.asInstanceOf[Unit]
    println(identity('a').asInstanceOf[Int])
    println('a'.asInstanceOf[Int])
    println(identity(1).asInstanceOf[Int])
    println(3L.asInstanceOf[Double])
    println(2.5.asInstanceOf[Byte])
    println(1.asInstanceOf[Any])
    println(1.asInstanceOf[Integer])
    println('c'.asInstanceOf[AnyRef])
    println(anyInt(4))
    println(new Nothings().assigned(false))

class Holder { lazy val s = null }

object Lazies:
  lazy val u: Unit = println("u")
  lazy val n = null
  lazy val x: Nothing = throw new Error("x")
  def run(): Unit =
    u
    val v: Unit = u
    println(v)
    println(n)
    println(new Holder().s)
    try x catch case e: Error => println(e.getMessage)

object Units:
  val g: Unit = println("g")
  def p(u: Unit): Any = u
  def q(b: Boolean, v: Unit): Unit = if b then v else g
  def run(): Unit =
    val v: Unit = println("v")
    println(v)
    val w = g
    println(w)
    var x: Unit = ()
    x = println("x")
    println(x)
    println(p(v))
    q(false, v)

object Tests:
  type T = X | Y
  def either(x: Object) = x.isInstanceOf[T]
  def both(x: Any) = x.isInstanceOf[TA & TB]
  def m(x: Any): String = x match
    case _: (TA & TB) => "tab"
    case _: TA => "ta"
    case _: (X | Y) => "xy"
    case _ => "none"
  lazy val any = { println(1); 1: Any }
  lazy val int = { println(2); 2 }
  def run(): Unit =
    println(either(new X))
    println(either(new Object))
    println(both(new TA with TB {}))
    println(m(new TA {}))
    println(m(new Y))
    any.isInstanceOf[Int]
    int.isInstanceOf[Int]
    val i: Int = 1
    println(i.isInstanceOf[Object])
    println(1.isInstanceOf[Object])
    println(i.isInstanceOf[Integer])
    println(identity(i).isInstanceOf[Int])
    Array(1) match { case _: Array[Int] => println("array") }
    val o: Any = Array("a")
    println(o.isInstanceOf[Array[String]])
    println(o.isInstanceOf[Array[Int]])
    println(o.isInstanceOf[Unit])

@main def exprforms(): Unit =
  Classes.run()
  Imports.run()
  Catches.run()
  Casts.run()
  Lazies.run()
  Units.run()
  Tests.run()
