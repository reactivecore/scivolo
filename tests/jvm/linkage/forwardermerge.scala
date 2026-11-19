package forwardermerge

class B:
  def m: Int = 1

trait T:
  def m: Int

object M extends B with T

trait I:
  def view: AnyRef = "i"

trait S extends I:
  override def view: String = "s"

abstract class AS extends S

trait L extends S

object W extends AS with L
