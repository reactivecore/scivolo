import scala.scalanative.unsafe._
import scala.scalanative.unsigned._

class Sink(protected var out: StringBuilder)

class Printer(o: StringBuilder) extends Sink(o) {
  private var closed = false
  private def trap(body: => Unit): Unit =
    try body
    catch { case _: IllegalStateException => out.append("!") }
  def flush(): Unit = synchronized {
    if (closed) () else trap(out.append("flushed;"))
  }
  def later(): () => Int = () => out.length
  def shutdown(how: Int)(onSuccess: => Unit): Unit = how match {
    case 0 => onSuccess
    case _ => throw new IllegalStateException(s"mode $how")
  }
}

object implicitthis {
  def main(args: Array[String]): Unit = {
    val sb = new StringBuilder
    val p = new Printer(sb)
    p.flush()
    p.shutdown(0)(sb.append("down;"))
    println(sb)
    println(p.later()())
    Zone.acquire { implicit z =>
      val n = 3.toUSize
      val buf: Ptr[Byte] = alloc[Byte](n)
      buf(1) = 7.toByte
      println(buf(0) + buf(1))
    }
  }
}
