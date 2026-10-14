import scala.language.implicitConversions

class Box(val s: String)
case class Card(number: Int) {
  def isInDeck(implicit deck: List[Card]) = deck contains this
}
object Views {
  given Conversion[Int, Box] with
    def apply(i: Int): Box = new Box(i.toString)
  given boolBox(using b: Box): Conversion[Boolean, Box] with
    def apply(x: Boolean): Box = b
  def a: Box = 1
  def b = (2: Box).s
  def c(using Box): Box = true
  def d = {
    implicit val deck: List[Card] = List(Card(1))
    implicit def intToCard(n: Int): Card = Card(n)
    1.isInDeck
  }
  object Widened {
    implicit object f1 extends (Int => String) { def apply(x: Int): String = "Int" }
    implicit def f2(x: Long): String = "Long"
    def e: String = 5
  }
  def f(x: Int) = {
    given () => Int = x
    var y = x
    y += 1
    y
  }
}
