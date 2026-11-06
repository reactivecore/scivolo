import scala.annotation.tailrec

trait Props {
  private def quietly(action: => Unit, disposal: => Unit) =
    try action
    finally disposal
  protected lazy val props: String = {
    val sb = new StringBuilder
    quietly(sb.append("a"), sb.append("b"))
    sb.toString
  }
  protected def initialCapacity: Int = 4
  var table: Array[String | Null] = new Array(initialCapacity)
}

final class RI(val self: Int) extends AnyVal {
  def num: scala.math.Numeric.IntIsIntegral.type = scala.math.Numeric.IntIsIntegral
}

final class Chain(val head: Int, val tail: Chain | Null) {
  private def mk(f: => Chain): Chain = f
  @tailrec def reverseOnto(tl: Chain): Chain =
    if (tail == null) new Chain(head, tl)
    else tail.nn.reverseOnto(mk(new Chain(head, tl)))
  @tailrec private def last: Int =
    if (tail == null) head else tail.nn.last
  def lastOne: Int = last
  def toList: List[Int] = head :: (if (tail == null) Nil else tail.nn.toList)
}

object positionsnames extends Props {
  private def onFailure(handle: Exception => Unit): PartialFunction[Throwable, Unit] = {
    case e: Exception => handle(e)
  }
  private def report(e: Exception): Unit = println("caught " + e.getMessage)

  def main(args: Array[String]): Unit = {
    println(props + " " + table.length)
    println(new RI(3).self)
    val c = new Chain(1, new Chain(2, new Chain(3, null)))
    println(c.reverseOnto(new Chain(0, null)).toList.toString + " " + c.lastOne)
    try throw new Exception("boom")
    catch onFailure(report)
  }
}
