object locallazyobjects {
  var log = List.empty[String]
  def note(s: String): Unit = log = s :: log

  def once(k: Int): Int = {
    object L {
      note(s"L init $k")
      lazy val a: Int = { note(s"a $k"); k * 10 }
      lazy val b: Int = { note(s"b $k"); a + 1 }
      val c: Int = 5
    }
    note("before")
    val r = L.b + L.a + L.b
    note("after")
    r
  }

  def recursive(n: Int): Int = {
    object R {
      lazy val fib: Int => Int = i => if (i < 2) i else fib(i - 1) + fib(i - 2)
      lazy val self: Int = n + capture
      def capture = n * 2
    }
    R.fib(n) + R.self
  }

  def nested(k: Int): Int = {
    object Outer {
      lazy val x: Int = k + 1
      object Inner {
        lazy val y: Int = x * 2
      }
    }
    Outer.Inner.y + Outer.x
  }

  def inClass(k: Int): Int = {
    class C(m: Int) {
      object O { lazy val v: Int = { note(s"v $m"); m + k } }
      def get = O.v + O.v
    }
    val c1 = new C(1)
    val c2 = new C(2)
    c1.get + c2.get + c1.get
  }

  def throwing(k: Int): Int = {
    var tries = 0
    object T {
      lazy val v: Int = { tries += 1; if (tries < 2) throw new RuntimeException("x") else k }
    }
    val first = try T.v catch { case _: RuntimeException => -1 }
    first + T.v + tries
  }

  def main(args: Array[String]): Unit = {
    println(once(1))
    println(once(2))
    println(log.reverse.mkString(","))
    println(recursive(10))
    println(nested(3))
    log = Nil
    println(inClass(10))
    println(log.reverse.mkString(","))
    println(throwing(7))
  }
}
