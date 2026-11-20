trait Foo[+A]:
  class Bar[B >: A]

class Test:
  def t1[X](foo: Foo[X]): Unit =
    val bar = new foo.Bar()
