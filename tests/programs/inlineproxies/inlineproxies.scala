//> using scala 3.9.0
import scala.scalanative.runtime.Continuations

class Parker(val nanos: Long) {
  type Boundary = Continuations.BoundaryLabel[Unit]
  var boundary: Boundary | Null = null
  var state = 0

  def park(): Unit =
    Continuations.suspend[Unit] { resume =>
      state = 1
      if (nanos > 0) println(s"resume $nanos")
    }(using boundary.asInstanceOf[Boundary])

  def parkAgain(): Unit =
    Continuations.suspend[Unit] { resume =>
      state = 2
    }(using boundary.asInstanceOf[Boundary])
}

object inlineproxies {
  def suspend[R, T](label: Long, call: Long => T): R = { call(label); null.asInstanceOf[R] }
  @noinline def fence(x: Any): Unit = ()

  def stop(): Unit =
    try suspend[Nothing, Unit](1L, _ => ())
    finally fence(this)

  def resumer(on: Boolean): () => Unit | Null = if (on) () => println("resumed") else () => null

  def resume(on: Boolean): Unit = {
    val continue = resumer(on)
    try continue()
    finally fence(continue)
  }

  def main(args: Array[String]): Unit = {
    val p = new Parker(3)
    if (args.length > 5) { p.park(); p.parkAgain(); stop() }
    resume(true)
    println(p.state)
  }
}
