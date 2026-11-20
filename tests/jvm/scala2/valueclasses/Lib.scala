package valueclasses

class Meter(val v: Double) extends AnyVal {
  def +(o: Meter): Meter = new Meter(v + o.v)
  def show: String = s"${v}m"
  def scaled[N](n: N)(implicit num: Numeric[N]): Meter = new Meter(v * num.toDouble(n))
}

object Meter {
  val zero: Meter = new Meter(0)
  def of(v: Double): Meter = new Meter(v)
}

case class Name(s: String) extends AnyVal {
  def greet: String = "hello " + s
}

class Hidden(private val n: Int) extends AnyVal {
  def twice: Int = n * 2
}

trait Shape extends Any {
  def area: Double
  def describe: String = "area " + area
}

class Square(val side: Double) extends AnyVal with Shape {
  def area: Double = side * side
}

class Box[A](val a: A) extends AnyVal {
  def map[B](f: A => B): Box[B] = new Box(f(a))
  def get: A = a
}

class Chunk[A](val xs: Array[A]) extends AnyVal {
  def size: Int = xs.length
}

class Holder(val m: Meter) {
  def next: Meter = m + new Meter(1)
}

object Ops {
  implicit class Shout(val s: String) extends AnyVal {
    def shout: String = s.toUpperCase + "!"
  }

  def sum(ms: Array[Meter]): Meter = ms.foldLeft(Meter.zero)(_ + _)
  def total(ms: List[Meter]): Double = ms.map(_.v).sum
  def boxed(b: Box[Int]): Int = b.a + 1
  def chunk(c: Chunk[String]): Int = c.size
  def shapes(s: Shape): String = s.describe
  def maybe(o: Option[Meter]): Meter = o.getOrElse(Meter.zero)
  def name(n: Name): String = n.s
  def id[T](t: T): T = t
}
