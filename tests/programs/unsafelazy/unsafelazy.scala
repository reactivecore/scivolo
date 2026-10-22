//> using scala 3.9.0
import scala.annotation.threadUnsafe

object Marker

trait Mixed {
  @threadUnsafe lazy val mixed: Int = { println("mixed"); 5 }
}

class Holder(n: Int) extends Mixed {
  @threadUnsafe lazy val twice: Int = { println("twice"); n * 2 }
  @threadUnsafe lazy val text: String = { println("text"); "n=" + n }
  @threadUnsafe lazy val marker: Marker.type = { println("marker"); Marker }
}

object unsafelazy {
  def local(k: Int): Int = {
    @threadUnsafe lazy val square: Int = { println("square"); k * k }
    square + square
  }

  def main(args: Array[String]): Unit = {
    val h = new Holder(21)
    println(h.twice + h.twice)
    println(h.text + h.text)
    println(h.marker eq h.marker)
    println(local(3))
    println(h.mixed + h.mixed)
  }
}
