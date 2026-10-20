object Even:
  inline def unapply(inline n: Int): Option[Int] = if n % 2 == 0 then Some(n / 2) else None

object Macro:
  opaque type StrCtx = StringContext
  def apply(ctx: StringContext): StrCtx = ctx

extension (ctx: StringContext) def mac: Macro.StrCtx = Macro(ctx)
extension (inline ctx: Macro.StrCtx) inline def unapplySeq(inline input: Int): Option[Seq[Int]] =
  Some(Seq(input))

trait Greeter:
  def greet(): String

def half(n: Int): String = n match
  case Even(h) => s"half $h"
  case _ => "odd"

@main def inlineextractors(): Unit =
  println(half(10))
  println(half(7))
  val mac"$x" = 42
  println(x + 1)
  val g = new Greeter:
    def greet() = "hello"
  println(g.greet())
  println(g.getClass.getName)
