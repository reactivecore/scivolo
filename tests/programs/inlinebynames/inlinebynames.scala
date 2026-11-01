import scala.scalanative.runtime.Continuations
import scala.scalanative.runtime.Continuations.BoundaryLabel

class Waiter {
  def first(using BoundaryLabel[Unit]): Unit =
    Continuations.suspend[Unit] { resume =>
      println("first")
      resume()
    }

  def second(using BoundaryLabel[Unit]): Unit =
    Continuations.suspend[Unit] { resume => println("second") }
}

object Waiter {
  class Loop {
    def run(ws: List[Waiter])(using BoundaryLabel[Unit]): Unit = {
      val step: Waiter => Unit = w => w.first
      ws.foreach(step)
    }
  }
}

object inlinebynames {
  def main(args: Array[String]): Unit = println(List(1, 2).map(x => x * 2))
}
