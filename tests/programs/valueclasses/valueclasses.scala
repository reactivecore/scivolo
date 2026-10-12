//> using scala 3.9.0
class Meter(val value: Double) extends AnyVal {
  def +(other: Meter): Meter = new Meter(value + other.value)
  def twice: Meter = this + this
  def show[T](t: T): String = s"$value m $t"
  override def toString: String = s"Meter($value)"
}

class Wrap[A](val a: A) extends AnyVal {
  def get: A = a
}

trait Shows extends Any {
  def text: String
  def shown: String = "<" + text + ">"
}

class Name(val s: String) extends AnyVal with Shows {
  def text: String = s
}

class Secret(private val code: Int) extends AnyVal {
  def reveal: Int = code * 2
}

class Half(n: Int) extends AnyVal {
  def half: Int = n / 2
}

object valueclasses {
  def len(n: Name): Int = n.s.length
  def main(args: Array[String]): Unit = {
    val m = new Meter(1.5)
    println(m + new Meter(2.0))
    println(m.twice.value)
    println(m.show(3))
    println(m == new Meter(1.5))
    println(m.hashCode)
    println(new Meter(2.5).value)
    val w = new Wrap(7)
    println(w.get + 1)
    val n = new Name("x")
    println(n.shown + len(n))
    println(new Secret(21).reveal)
    val h = new Half(9)
    println(h.half)
    println(h == new Half(8))
    println(3.compare(4))
  }
}
