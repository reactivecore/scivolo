class Foo

extension (dfVal: Foo)
  def f1[T](step: Int): Foo = ???
  def f1: Foo = ???

extension (dfVal: Foo)
  def f2[T](step: Int): Foo = ???
  def f2[T]: Foo = ???

object Test:
  val v1 = (new Foo).f1
  val v2 = (new Foo).f2
  val v3 = (new Foo).f1[Int](1)
