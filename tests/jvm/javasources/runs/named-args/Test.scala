object Test {
  def main(args: Array[String]): Unit = {
    println(Greeter.greet(name = "World", greeting = "Hello"))
    println(new Greeter().twice(word = "ho", times = 3))
    println(Greeter.Color.valueOf(x = "RED"))
  }
}
