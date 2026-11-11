@FunctionalInterface
trait Inc { def apply(x: Int): Int }

class Names {
  @java.lang.SafeVarargs final def count(xs: String*): Int = xs.length
}

object Javaannotations {
  def main(args: Array[String]): Unit = {
    val inc: Inc = x => x + 1
    println(inc(1))
    println(new Names().count("a", "b"))
    println(classOf[Inc].isAnnotationPresent(classOf[FunctionalInterface]))
    val count = classOf[Names].getMethods.filter(_.getName == "count")
    println(count.map(_.isAnnotationPresent(classOf[java.lang.SafeVarargs])).mkString)
  }
}
