import java.lang.annotation.Documented

@FunctionalInterface
trait Inc { def apply(x: Int): Int }

class Names {
  @java.lang.SafeVarargs final def count(xs: String*): Int = xs.length
  @Documented val size = 2
}

object Javaannotations {
  @Documented def kept = 3
  @SuppressWarnings(Array("unused")) def quiet = 4
  @Deprecated def old = 5
  @Deprecated(since = "9", forRemoval = true) def older = 6

  def deprecation(name: String): String = {
    val d = Class.forName("Javaannotations").getMethod(name).getAnnotation(classOf[Deprecated])
    s"[${d.since()}] ${d.forRemoval()}"
  }

  def main(args: Array[String]): Unit = {
    val inc: Inc = x => x + 1
    println(inc(1))
    println(new Names().count("a", "b"))
    println(classOf[Inc].isAnnotationPresent(classOf[FunctionalInterface]))
    val count = classOf[Names].getMethods.filter(_.getName == "count")
    println(count.map(_.isAnnotationPresent(classOf[java.lang.SafeVarargs])).mkString)
    val names = classOf[Names]
    println(names.getDeclaredField("size").isAnnotationPresent(classOf[Documented]))
    println(names.getMethod("size").isAnnotationPresent(classOf[Documented]))
    val forwarder = Class.forName("Javaannotations").getMethod("kept")
    println(forwarder.isAnnotationPresent(classOf[Documented]))
    println(quiet)
    println(deprecation("old") + " " + deprecation("older"))
  }
}
