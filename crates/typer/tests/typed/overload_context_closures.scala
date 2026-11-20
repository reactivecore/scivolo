import scala.annotation.targetName

trait Ctx

object OverloadContextClosures:
  def foo(f: Ctx => Int) = 1
  @targetName("fooContextual")
  def foo(f: Ctx ?=> Int) = 2

  def plain = foo(ctx => 123)
  def contextual = foo(ctx ?=> 123)

  class C:
    def m(a: Nothing): Int = 0
  implicit class RichAny(a: Any):
    def m(a: Any): Int = 1
  def ascribed = (new C).m({ case (x, y) => x }: Any => Any)
