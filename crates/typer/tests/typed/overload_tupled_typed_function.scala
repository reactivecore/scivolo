trait Foo:
  def g(f: ((Int, Int)) => Int): Int = 1
  def g(f: ((Int, Int)) => (Int, Int)): String = "2"

object Test:
  def m(foo: Foo) =
    val a = foo.g((x: Int, b: Int) => (x, x))
    val b = foo.g((x: Int, b: Int) => x + b)
    (a, b)
