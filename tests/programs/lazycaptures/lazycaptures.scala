class Spawn {
  def spawn(grace: Long, destroy: Boolean): String = {
    val label = "p" + grace
    lazy val hook = if (!destroy) None else Some(new Object { override def toString = proc + grace })
    lazy val monitor = hook.map(t => new Object { override def toString = proc + (t eq null) })
    lazy val proc: String = label + monitor.isDefined
    proc + hook.isDefined
  }
}

object lazycaptures {
  def main(args: Array[String]): Unit = {
    println(new Spawn().spawn(1, false))
    println(new Spawn().spawn(2, true))
  }
}
