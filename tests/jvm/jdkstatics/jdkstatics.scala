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
  }
}
