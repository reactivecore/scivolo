import scala.annotation.static

trait Base:
  def g: Int = 5

object Base:
  @static val x = 10
  @static final val y = 11
  @static def f: Int = x + 20

class Impl extends Base

@main def Lw30statictrait(): Unit =
  println(Base.x)
  println(Base.y)
  println(Base.f)
  println(Impl().g)
