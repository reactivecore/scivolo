import scala.scalanative.unsafe._

object Groups {
  object Values {
    object Unsupported
  }
}

class Base(a: AnyRef, b: Int, c: Boolean) {
  override def toString = s"${a != null} $b $c"
}

class Holder(n: Int) extends Base(Groups.Values.Unsupported, n, true) {
  var a: AnyRef = null
  var b: AnyRef = null

  def this(s: String, c: Int) = {
    this(c)
    def flag(f: Int) = (c & f) != 0
    if (flag(1)) {
      a = Groups.Values.Unsupported
      this.b = Groups.Values
    }
  }

  def set(): Unit = {
    a = Groups.Values.Unsupported
    if (a != null && a != Groups.Values.Unsupported) b = a
  }

  def both(o: AnyRef): Boolean = {
    var k: AnyRef = null
    var v: AnyRef = null
    { k = o.toString; k } != null && { v = o; v } != null && (k ne v)
  }
}

object positionsscopes {
  implicit class RichByte(val b: Byte) extends AnyVal {
    @inline def size: Size = b.toInt.toSize
  }

  def half(p: Int): Int = (p / 2).max(1) + (p).max(2)

  def main(args: Array[String]): Unit = {
    val h = new Holder("x", 3)
    h.set()
    println(h.a == Groups.Values.Unsupported)
    println(h.both("o"))
    println(half(args.length + 7))
    println(3.toByte.size.toInt)
    println(h)
  }
}
