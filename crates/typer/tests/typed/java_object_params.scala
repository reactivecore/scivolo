import java.util.Objects

object Test {
  def main(args: Array[String]): Unit = {
    println(Objects.equals(1, 1))
    println(Objects.hashCode('c'))
  }
}
