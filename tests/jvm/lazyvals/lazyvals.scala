class Box(n: Int) {
  lazy val twice: Int = { println("twice"); n * 2 }
  lazy val name: String = { println("name"); "box" + n }
  lazy val none: String = null
}

object Cache {
  lazy val answer: Int = { println("answer"); 42 }
  lazy val words: List[String] = List("a", "b")
}

object Lazyvals {
  def local(n: Int): Int = {
    lazy val square = { println("square"); n * n }
    square + square
  }

  def main(args: Array[String]): Unit = {
    val b = new Box(3)
    println(b.twice + b.twice)
    println(b.name + b.name)
    println(b.none == null)
    println(Cache.answer + Cache.answer)
    println(Cache.words.length)
    println(local(4))
  }
}
