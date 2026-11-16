import java.lang.annotation.{Documented, Inherited, Retention, RetentionPolicy}

@FunctionalInterface
trait Inc { def apply(x: Int): Int }

class Names {
  @java.lang.SafeVarargs final def count(xs: String*): Int = xs.length
  @Documented val size = 2
}

object Policies {
  final val Runtime = RetentionPolicy.RUNTIME
}

trait Mixed {
  @Documented @Inherited @Deprecated def m(@Documented @Deprecated a: Int) = a
}

@Documented @Inherited @Deprecated
class Several(@Documented @Deprecated p: Int, @Documented @Deprecated val q: Int) extends Mixed {
  @Documented @Inherited @Deprecated def three = p
  @Documented @Inherited @Deprecated val f3 = 4
  def params(@Deprecated @Documented a: Int, @Documented @Inherited @Deprecated b: Int) = a + b
  @Retention(Policies.Runtime) def aliased = 6
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
    several()
  }

  def names(as: Array[java.lang.annotation.Annotation]): String =
    as.map(_.annotationType.getSimpleName).mkString(" ")

  def several(): Unit = {
    val c = classOf[Several]
    println(names(c.getAnnotations))
    println(names(c.getMethod("three").getAnnotations))
    println(names(c.getDeclaredField("f3").getAnnotations))
    println(names(c.getMethod("m", classOf[Int]).getAnnotations))
    println(c.getMethod("m", classOf[Int]).getParameterAnnotations.map(names).mkString(", "))
    println(c.getMethod("params", classOf[Int], classOf[Int]).getParameterAnnotations.map(names).mkString(", "))
    println(c.getConstructors.head.getParameterAnnotations.map(names).mkString(", "))
    println(c.getDeclaredFields.filter(f => f.getName == "p" || f.getName == "q").map(_.getAnnotations.length).mkString)
    println(c.getMethod("aliased").getAnnotations.mkString)
    println(new Several(1, 2).three + new Several(1, 2).params(1, 2))
  }
}
