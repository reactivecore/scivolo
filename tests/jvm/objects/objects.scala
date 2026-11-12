class Base(val tag: String) { def who: String = tag; private def secret: String = "secret"; def tell: String = secret }
class Mid(t: String) extends Base(t)
class Leaf(t: String) extends Mid(t) { override def who: String = super.who; override def toString: String = "Leaf!" }

object Other {
  val name: String = "other"
  var count: String = "zero"
  def hello(s: String): Unit = println(s)
}

object Objects {
  def leaf: Leaf = new Leaf("tagged")
  def show(a: String, b: String): String = { println(a); b }
  def nested(s: String): String = { class L { def f = s }; new L().f }
  def main(args: Array[String]): Unit = {
    println(leaf.who)
    println(leaf.tell)
    println(leaf.toString)
    println(show("a", "b"))
    println(Other.name)
    Other.count = "one"
    println(Other.count)
    Other.hello("hello")
    println(nested("local"))
    println(classOf[Int])
    println(classOf[Unit])
    println(classOf[Array[String]])
    println(classOf[Meter])
    println(Array(Meter(1.5), Meter(2.5)).map(_.d).toList)
    object Local
    println(java.lang.reflect.Modifier.isFinal(Local.getClass.getModifiers))
  }
}

class Meter(val d: Double) extends AnyVal
