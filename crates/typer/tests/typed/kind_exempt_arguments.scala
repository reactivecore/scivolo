import scala.annotation.StaticAnnotation

class meh extends StaticAnnotation

final class Bar[+F[_]]
object Bar:
  def empty[F[_]]: Bar[F] = new Bar[Nothing]
  def nothing: Bar[Nothing] = empty[Nothing]

class ALike[C]
abstract class AFactory[CC[x] <: ALike[CC[x]]]:
  def aCompanion: AFactory[CC @meh]

type K[T <: AnyKind] = T
val a: K[Int] = 1
val b: K[List][Int] = Nil
