object InlineArgSkolems:
  def inferred = summon(using Option(1))
  def explicit = summon[Option[Int]](using Option(1))
  def adapted = summon[Int =:= Int](using <:<.refl)
  def cast = summon[Int =:= Int](using <:<.refl.asInstanceOf[Int =:= Int])
  def block = summon[Option[Int]](using { val x = 1; Option(x) })
