//> using scala 3.9.0
// Protected methods of javalib classes used from inner, anonymous and local classes of their
// subclasses, and from classes in traits.

class Cache extends java.util.LinkedHashMap[String, Int] {
  class Probe { def evicts(e: java.util.Map.Entry[String, Int]): Boolean = removeEldestEntry(e) }
  def check(): Boolean = {
    val task = new java.util.function.Supplier[Boolean] { def get(): Boolean = removeEldestEntry(null) }
    task.get()
  }
  def local(k: Int): String = {
    class Look { def get = s"${removeEldestEntry(null)} $k" }
    new Look().get
  }
}

class Counter extends ThreadLocal[String] {
  override protected def initialValue(): String = "init"
  class Init { def first = initialValue() + "!" }
}

class Nums(n: Int) extends java.util.AbstractList[Int] {
  def get(i: Int): Int = i * 10
  def size(): Int = n
  class Cut { def cut(from: Int, to: Int): Unit = removeRange(from, to) }
}

trait Ranged extends java.util.AbstractList[String] {
  class Trim { def trim(): Unit = removeRange(0, 1) }
}

class Words extends java.util.ArrayList[String] with Ranged

trait Copies extends Cloneable { def copy(): Object = clone() }

class Sheep extends Copies

object protectedaccess {
  def main(args: Array[String]): Unit = {
    val c = new Cache
    c.put("a", 1)
    println(s"${new c.Probe().evicts(null)} ${c.check()} ${c.local(2)}")
    val t = new Counter
    println(new t.Init().first + " " + t.get())
    val n = new Nums(3)
    new n.Cut().cut(1, 1)
    println(n.get(2) + n.size())
    val w = new Words
    w.add("a")
    w.add("b")
    new w.Trim().trim()
    println(w.size() + " " + w.get(0))
    val sheep = new Sheep
    println(sheep.copy().getClass.getName + " " + (sheep.copy() ne sheep))
  }
}
