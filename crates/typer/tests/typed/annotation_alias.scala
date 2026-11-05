import Pool.*

class Pool:
  @Marked("ctl") @volatile var ctl: Long = 0

object Pool:
  private type Marked = scala.deprecated
