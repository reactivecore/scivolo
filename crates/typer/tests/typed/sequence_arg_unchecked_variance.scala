object S:
  def count[A](as: A*): Int = if as.isEmpty then 0 else 1 + count(as.tail*)
