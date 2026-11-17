package scala.probe

import language.experimental.captureChecking

trait Appending[A] extends Iterable[A]:
  def twice(suffix: IterableOnce[A]^): Iterable[A]^{this, suffix} = super.concat(suffix)

object LibCaptures:
  def buildFrom[A, CC[_]](factory: collection.IterableFactory[CC]): collection.BuildFrom[Any, A, CC[A]] =
    new collection.BuildFrom[Any, A, CC[A]]:
      def fromSpecific(from: Any)(it: IterableOnce[A]^) = factory.from(it)
      def newBuilder(from: Any) = factory.newBuilder

object Providers:
  def provider[A](it: () => Iterator[A]^): Iterable[A]^{it} = new collection.AbstractView[A]:
    def iterator = it()
