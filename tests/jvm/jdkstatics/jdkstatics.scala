import java.nio.file.attribute.PosixFilePermission
import java.util.concurrent.TimeUnit

object Jdkstatics {
  def main(args: Array[String]): Unit = {
    System.out.println(String.valueOf(12) + String.format("|%,d|%.2f", Integer.valueOf(1234567), 3.14159))
    println(String.CASE_INSENSITIVE_ORDER.compare("a", "A") + Integer.parseInt("42"))
    println(s"${Character.isDigit('1')} ${java.lang.Integer.TYPE} ${Math.max(1, 2)}")
    println(java.util.Collections.EMPTY_LIST.toString + java.util.Collections.emptyList[String]())
    println(TimeUnit.SECONDS.toMillis(1) + java.util.Comparator.naturalOrder[Integer]().compare(1, 2))
    println(java.nio.charset.StandardCharsets.UTF_8.toString + " " + PosixFilePermission.values.length)
    val ints = new java.util.ArrayList[Int]()
    java.util.Collections.addAll(ints, 5, 6)
    println(ints.toString + added("a", "b"))
  }

  def added[A](a1: A, a2: A): java.util.List[A] = {
    val coll = new java.util.ArrayList[A]()
    java.util.Collections.addAll(coll, a1, a2)
    coll
  }
}
