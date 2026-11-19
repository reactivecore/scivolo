object prelude:
  export Semigroup.*

trait Semigroup[A]:
  def scombine(x: A, y: A): A

object Semigroup:
  def scombine[A](x: A, y: A) = (s: Semigroup[A]) ?=> s.scombine(x, y)

object Use:
  given Semigroup[Int] with
    def scombine(x: Int, y: Int): Int = x + y
  def three: Int = prelude.scombine(1, 2)
