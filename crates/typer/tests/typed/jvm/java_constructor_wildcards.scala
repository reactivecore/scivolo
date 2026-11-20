class JavaConstructorWildcards:
  def copied(xs: java.util.List[String]) = new java.util.ArrayList[CharSequence](xs)
