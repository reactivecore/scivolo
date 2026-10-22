object Main {
  def main(args: Array[String]): Unit = {
    val name = classOf[Main.type].getName
    val tail = "abc".substring(1)
    val parts = "a,b".split(",")
    println(name == "Main$")
    println(tail.length + parts.length)
    println(String.valueOf(42))
  }
}
