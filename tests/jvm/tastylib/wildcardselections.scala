class WOuter[T] { class In; type Same = String }
class WBox[T]
class WSel[A] extends WBox[WOuter[A]#In]
class WAlias[A] extends WBox[WOuter[A]#Same]

object WildcardSelections {
  def id[T](b: WBox[T]): WBox[T] = b
  def bounded(s: WSel[?]): WBox[? <: WOuter[?]#In] = s
  def alias(s: WAlias[?]): WBox[String] = s
  def inferredAlias(s: WAlias[?]) = id(s)
}
