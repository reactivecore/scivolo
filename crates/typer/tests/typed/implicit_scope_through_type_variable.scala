object ImplicitScopeThroughTypeVariable {
  class Parent { class E }
  object ChildA extends Parent

  class Printer[C <: Parent](val child: C) {
    def print(e: child.E): String = ""
  }

  trait Trait { case class Dependent() }
  object obj extends Trait
  case class Dep[T <: Trait](t: T) {
    def fun(q: t.Dependent): Unit = ()
  }

  def test = {
    Printer(ChildA).print(new ChildA.E)
    Dep(obj).fun(obj.Dependent())
  }
}
