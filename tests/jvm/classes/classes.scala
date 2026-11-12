abstract class Shape(val name: String) {
  def area: Double
  def describe(): String = name
}

class Rect(w: Double, h: Double) extends Shape("rect") {
  def area: Double = w * h
  override def describe(): String = super.describe()
}

class Counter {
  var count: Int = 0
  def inc(): Unit = count = count + 1
}

class Record(val key: String) {
  def child(k: String): Record = Record(s"$key.$k")
}

object Classes {
  def main(args: Array[String]): Unit = {
    val r = new Rect(2.0, 3.0)
    println(r.describe())
    println(r.area)
    val s: Shape = r
    println(s.area)
    val c = new Counter
    c.inc()
    c.inc()
    println(c.count)
    println(Record("a").child("b").key)
  }
}
