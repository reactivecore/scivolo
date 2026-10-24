import scala.scalanative.unsafe.*

object ptrassign {
  def set(p: Ptr[Int]): Unit = !p = 3
  def setFirst(p: Ptr[CStruct2[Int, Long]]): Unit = p._1 = 4

  def main(args: Array[String]): Unit = println("ok")
}
