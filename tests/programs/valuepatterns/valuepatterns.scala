//> using scala 3.9.0
class Meter(val value: Double) extends AnyVal {
  override def toString: String = s"Meter($value)"
}

trait Shows extends Any {
  def text: String
  def shown: String = "<" + text + ">"
}

class Name(val s: String) extends AnyVal with Shows {
  def text: String = s
}

class Count(val n: Int) extends AnyVal

case class Cents(c: Int) extends AnyVal

final class Opt[A](val x: A) extends AnyVal {
  def isEmpty: Boolean = x == null
  def get: A = x
}

object Even {
  def unapply(i: Int): Opt[String] = new Opt(if (i % 2 == 0) "even " + i else null)
}

object Far {
  def unapply(m: Meter): Boolean = m.value > 2.0
}

object Halved {
  def unapply(m: Meter): Option[Meter] = if (m.value > 0) Some(new Meter(m.value / 2)) else None
}

object valuepatterns {
  def isMeter(o: Any): Boolean = o.isInstanceOf[Meter]
  def self(m: Meter): Boolean = m.isInstanceOf[Meter]
  def selfMatch(m: Meter): Double = m match {
    case x: Meter => x.value
  }
  def shows(n: Any): String = n match {
    case s: Shows => s.shown
    case _ => "none"
  }
  def cents(o: Any): Int = o match {
    case Cents(c) => c
    case _ => -1
  }
  def count(o: Any): Int = o match {
    case c: Count if c.n > 1 => c.n
    case c: Count => -c.n
    case _ => 0
  }
  def even(i: Int): String = i match {
    case Even(s) => s
    case _ => "odd"
  }
  def length(m: Meter): String = m match {
    case Far() => "far"
    case Halved(h) => "half " + h.value
    case _ => "none"
  }
  def main(args: Array[String]): Unit = {
    val m = new Meter(1.5)
    println(isMeter(m))
    println(isMeter(new Name("n")))
    println(self(m))
    println(selfMatch(m))
    println(shows(new Name("x")))
    println(cents(Cents(7)))
    println(cents(m))
    println(count(new Count(3)))
    println(count(new Count(1)))
    println(count("c"))
    println(even(4))
    println(even(3))
    println(length(new Meter(3.0)))
    println(length(m))
    println(length(new Meter(-1.0)))
    val i: Int = null.asInstanceOf[Int]
    println(i)
    val c: Count = null.asInstanceOf[Count]
    println(c.n)
    val o: Any = new Count(5)
    println(o.asInstanceOf[Count].n)
    println(m.asInstanceOf[Meter].value)
    println(m.asInstanceOf[Any])
    val wrong =
      try { 3.asInstanceOf[Count].n; "cast" }
      catch { case _: ClassCastException => "class cast" }
    println(wrong)
    val ms = new Array[Meter](3)
    ms(0) = m
    ms(1) = new Meter(2.5)
    ms(2) = ms(0)
    var k = 0
    var sum = 0.0
    while (k < ms.length) {
      sum += ms(k).value
      k += 1
    }
    println(sum)
    val cs = Array[Any](new Count(1), "s", Cents(2))
    println(cs(0).isInstanceOf[Count] && !cs(2).isInstanceOf[Count])
    println(cs(0).asInstanceOf[Count].n)
    val both: Name & Shows = new Name("i")
    val anyName: Any = both
    println(anyName.asInstanceOf[Name & Shows].text)
    println(new Opt[String]("o").asInstanceOf[Opt[Int]].x)
  }
}
