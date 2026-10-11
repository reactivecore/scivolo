//> using scala 3.9.0
import java.io.{ OutputStream, PrintStream }

abstract class Counted(label: => String, start: Int, step: Int):
  def show: String = label + ":" + start + "+" + step

object Named extends Counted(step = { println("step"); 2 }, start = { println("start"); 1 }, label = "named")

class Base(val v: Int):
  println("base " + this.v)

class Over(override val v: Int) extends Base(v * 10)

trait Sink:
  val out = new PrintStream(new OutputStream { def write(b: Int) = () }) {
    override def println(x: Any) = ()
  }

class Wrap(a: Any):
  override def toString = "Wrap(" + a + ")"

@main def localclasses2(): Unit =
  println(Named.show)
  new Over(4)
  case class Point(x: Int, y: Int)
  println(Point(1, 2) == Point(1, 2))
  val k = 5
  def scale(x: Int) = x * k
  class Scaled { val a = scale(2); def b = scale(3) }
  val s = new Scaled
  println(s.a + s.b)
  object Box { def make = new Box }
  class Box { def size = scale(1) }
  println(Box.make.size)
  class Holder { def inner = new Scaled }
  println(new Holder().inner.a)
  class Parent(p: Any) { override def toString = "Parent(" + p + ")" }
  class Child extends Parent({ val w = new Wrap(k) {}; w })
  println(new Child)
  class Anon extends Wrap(new Object { override def toString = "o" })
  println(new Anon)
  println(new Sink {}.out != null)
