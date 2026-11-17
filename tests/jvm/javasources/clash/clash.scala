package q

class C {
  def size: Int = 1
}

object Test {
  def main(args: Array[String]): Unit = println(new C().size)
}
