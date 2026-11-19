import language.experimental.erasedDefinitions

class C(x: Int):
  erased val e = x
class D:
  val c = C(22)
  export c.*
  erased val x = e

case class Foo(x: Int = 0)
extension (x: Any)
  private def foo = Foo
  export foo.apply
