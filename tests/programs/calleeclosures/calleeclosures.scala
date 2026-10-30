class Impl(val p: String, val l: Int)

abstract class Base {
  type Mapping = Map[String, (String, Int) => Impl]
  val known: Mapping
}

class Provider extends Base {
  val known: Mapping = {
    def View = (p, l) => new Impl(p, l)
    Map("a" -> View, "b" -> View)
  }
}

trait Fn2[T, U, R] { def apply(t: T, u: U): R }

trait Pred[T] { def test(t: T): Boolean }

class Ops[A](xs: List[A]) { def forall(f: Pred[A]): Boolean = xs.forall(f.test(_)) }

class Holder {
  def contains(o: Any): Boolean = o != null
  def all(c: List[?]): Boolean = new Ops(c).forall(contains)
}

class Zone

object calleeclosures {
  def code(): Int = 3
  def acquire[T](f: Zone => T): T = f(new Zone)
  def withFileOpen[T](path: String, access: Int, share: Int = 1, allow: Boolean = false)(
      fn: Int => T
  )(implicit z: Zone): T = fn(path.length + access + share + (if (allow) 100 else 0))

  def exists(p: String): Boolean =
    acquire { implicit z =>
      if (p.isEmpty) false
      else withFileOpen(p.trim, access = code(), allow = true)(_ > 106)
    }

  def pick(b: Boolean): Int = {
    val impl =
      if (b) Later.f(_, _)
      else Later.g(_, _)
    impl(2, "xyz")
  }

  def cmp(a: Boolean, b: Boolean): Int = if (a == b) 0 else if (a) 1 else -1
  private def compareImpl[T](a: Array[T], b: Array[T], f: => Fn2[T, T, Int]): Int = f.apply(a(0), b(0))
  def compare(a: Array[Boolean], b: Array[Boolean]): Int = compareImpl(a, b, cmp)

  def main(args: Array[String]): Unit = {
    val v = new Provider().known("a")("q", 3)
    println(s"${v.p} ${v.l}")
    println(exists(" abc "))
    println(exists("ab"))
    println(pick(true) + " " + pick(false))
    println(compare(Array(true), Array(false)) + " " + new Holder().all(List(1, "a")))
  }
}

object Later {
  def f(x: Int, y: String): Int = x + y.length
  def g(x: Int, y: String): Int = x * 10
}
