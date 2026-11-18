class WOuter[T] { class In; type Same = String }
class WBox[T]
class WHigher[F[_]]
class WInv[A, X]
class WSel[A] extends WBox[WOuter[A]#In]
class WAlias[A] extends WBox[WOuter[A]#Same]
class WLam[A] extends WHigher[[X] =>> WInv[A, X]]

object WildcardSelections {
  def id[T](b: WBox[T]): WBox[T] = b
  def bounded(s: WSel[?]): WBox[? <: WOuter[?]#In] = s
  def alias(s: WAlias[?]): WBox[String] = s
  def inferredAlias(s: WAlias[?]) = id(s)
  def over(b: WBox[WOuter[?]#In]): Int = 1
  def over(a: Any): Int = 2
  def picked(s: WSel[?]) = over(s)
  def overLam(h: WHigher[[X] =>> WInv[?, X]]): Int = 1
  def overLam(a: Any): Int = 2
  def pickedLam(s: WLam[?]) = overLam(s)
}
