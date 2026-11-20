object EqualityInstantiates:
  def pair: (Int, Set[Any]) = (1, Set(Some("x")))
  def same: Boolean = pair == (1, Set(Some("x")))
  def none: Boolean = Set[Any](None) == Set(None)
