import scala.scalanative.annotation.{alwaysinline, nooptimize, nospecialize, safePublish, stub}
import scala.scalanative.unsafe.{define, link, linkCppRuntime}

@link("m") @define("NATIVE_ANNOTATIONS") object Lib {
  @alwaysinline def a(x: Int): Int = x + 1
  @noinline @nooptimize def b(x: Int): Int = x * 2
  @nospecialize def c(x: Int): Int = x - 3
  @inline def d(x: Int): Int = x / 2
  @stub def unused(): Int = 0
  @link("c") @linkCppRuntime def e(x: Int): Int = x % 5
}

class Fields(@safePublish val a: Int) {
  @volatile var b: Int = a
  val c: Int = a + 1
}

@safePublish class Published(val x: Int) {
  @volatile var y: Int = x
}

object nativeannotations {
  def main(args: Array[String]): Unit = {
    println(Lib.a(1) + Lib.b(2) + Lib.c(3) + Lib.d(4) + Lib.e(7))
    val f = new Fields(1)
    f.b += f.c
    println(f.a + f.b + new Published(4).y)
  }
}
