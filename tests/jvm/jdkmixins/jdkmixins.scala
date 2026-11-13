import java.util.Comparator
import java.util.function.{Function => JFunction}

class ByLength extends Comparator[String] {
  def compare(a: String, b: String): Int = a.length - b.length
}

class Counts extends java.util.Map[String, Int] {
  private val m = new java.util.HashMap[String, Int]()
  def size(): Int = m.size()
  def isEmpty(): Boolean = m.isEmpty()
  def containsKey(k: Any): Boolean = m.containsKey(k)
  def containsValue(v: Any): Boolean = m.containsValue(v)
  def get(k: Any): Int = m.get(k)
  def put(k: String, v: Int): Int = m.put(k, v)
  def remove(k: Any): Int = m.remove(k)
  def putAll(o: java.util.Map[? <: String, ? <: Int]): Unit = m.putAll(o)
  def clear(): Unit = m.clear()
  def keySet(): java.util.Set[String] = m.keySet()
  def values(): java.util.Collection[Int] = m.values()
  def entrySet(): java.util.Set[java.util.Map.Entry[String, Int]] = m.entrySet()
}

class Tagged(out: java.io.OutputStream) extends java.io.PrintStream(out) {
  override def format(f: String, args: Object*): java.io.PrintStream = super.format("<" + f + ">", args*)
}

object Jdkmixins {
  def main(args: Array[String]): Unit = {
    val c = new ByLength
    val reversed: JFunction[String, String] = s => s.reverse
    println(c.thenComparing(reversed).compare("ab", "ba"))
    println(c.reversed().compare("a", "bb"))
    val m = new Counts
    m.put("a", 1)
    m.merge("a", 5, (x, y) => x + y)
    m.forEach((k, v) => println(s"$k=$v"))
    println(classOf[Counts].getMethods.exists(_.getName == "usingEntry"))
    val bytes = new java.io.ByteArrayOutputStream()
    val p: java.io.PrintStream = new Tagged(bytes)
    p.printf("%s-%s", "a", "b").flush()
    val forwarder = classOf[Tagged].getMethod("format", classOf[String], classOf[Array[Object]])
    println(bytes.toString + " " + forwarder.isVarArgs)
  }
}
