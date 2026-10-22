//> using scala 3.9.0
class Words(val words: List[String]) extends Prepending

trait Prepending { self: Words =>
  def add(w: String): Words = new Words(w :: self.words)
}

trait Appending { self: Words =>
  override def add(w: String): Words = new Words(self.words :+ w)
}

trait Suite {
  def run(): String = "suite"
}

trait SuiteMixin { this: Suite =>
  def run(): String
}

trait Around extends SuiteMixin { this: Suite =>
  abstract override def run(): String = "[" + super.run() + "]"
}

class Spec extends Suite with Around

object selfoverrides {
  def main(args: Array[String]): Unit = {
    val w = new Words(List("a")) with Appending
    println(w.add("b").words)
    println(new Words(List("a")).add("b").words)
    println(new Spec().run())
  }
}
