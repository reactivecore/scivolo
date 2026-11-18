import named.Named

object NamedParams {
  def main(args: Array[String]): Unit = {
    println(Named.minus(right = 2, left = 5))
    println(new Named(second = 3, first = 1).value)
  }
}
