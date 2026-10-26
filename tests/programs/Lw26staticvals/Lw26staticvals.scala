import scala.annotation.static

class Base

object Base:
  @static val x = 10
  @static final val y = 11
  @static var a = 12
  @static final var b = 13
  @static def f: Int = 30

@main def Lw26staticvals(): Unit =
  println(Base.x)
  println(Base.y)
  Base.a = 20
  println(Base.a)
  println(Base.b)
  println(Base.f)
