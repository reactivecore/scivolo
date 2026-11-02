trait Foo[A <: Foo[A]]
trait FooCreator[A <: Foo[A]] {
  def createFoo(): A
}

trait FooWrapper {
  type A <: Foo[A]
  def foo: A
}
object FooWrapper {
  def apply[A0 <: Foo[A0]](toWrap: A0): FooWrapper { type A = A0 } = new FooWrapper {
    type A = A0
    def foo: A0 = toWrap
  }
}

trait FooCreatorWrapper {
  type A <: Foo[A]
  def fooCreator: FooCreator[A]
}

case class Baz(wrapper: FooCreatorWrapper)

object Use {
  def process(bar: Baz): FooWrapper = bar match {
    case Baz(wrapper) => FooWrapper(wrapper.fooCreator.createFoo())
  }
  def created(bar: Baz) = bar match {
    case Baz(wrapper) => wrapper.fooCreator.createFoo()
  }
}
