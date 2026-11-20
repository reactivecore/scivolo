trait A { type T }
trait B { type T }

abstract class F {
  val arg: A
  val res: B { type T = arg.T } = new B { type T = arg.T }
}

object Test {
  val a = new A { type T = String }
  val b: B { type T = String } = {
    val tmp = new F { val arg: A { type T = String } = a }
    tmp.res
  }
}
