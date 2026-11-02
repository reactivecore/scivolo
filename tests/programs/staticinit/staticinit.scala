import scala.annotation.static
class Base
object Base:
  println("Base object")
  @static val x = { println("init x"); 1 }
object staticinit:
  def main(args: Array[String]): Unit =
    println("start")
    println(Base.x)
    println(Base.x + 1)
