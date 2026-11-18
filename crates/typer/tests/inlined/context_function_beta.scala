class Label

object M:
  inline def scoped[T](inline body: Label ?=> T): T =
    given Label = new Label
    body

object Test:
  def label(using l: Label): Label = l
  val a = M.scoped(label)
