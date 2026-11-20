class Hi
class Lo extends Hi
class Cov[+T >: Lo <: Hi]
class Contra[-T >: Lo <: Hi]
class Indir[T <: String, U <: T] {
  def foo: Indir[?, ?] = null
}

object WildcardParamBounds {
  val cov: List[Cov[Hi]] = ??? : List[Cov[?]]
  val contra: List[Contra[Lo]] = ??? : List[Contra[?]]
  def indir: Indir[? <: String, ? <: String] = (new Indir[String, String]).foo
}
