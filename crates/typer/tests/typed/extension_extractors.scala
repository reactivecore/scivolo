object Macro:
  opaque type StrCtx = StringContext
  def apply(ctx: StringContext): StrCtx = ctx

object Interpolators:
  extension (ctx: StringContext) def mac: Macro.StrCtx = Macro(ctx)
  extension [T](inline ctx: Macro.StrCtx) inline def unapplySeq[U](inline input: T | U): Option[Seq[T | U]] =
    Some(Seq(input))

class Box(val value: Int)

object Boxed

object Extractors:
  extension (b: Boxed.type) def unapply(x: Box): Option[Int] = Some(x.value)

object Main:
  def interpolated(n: Int): Int =
    import Interpolators.*
    n match
      case mac"$x" => x
  def boxed(b: Box): Int =
    import Extractors.*
    b match
      case Boxed(v) => v
