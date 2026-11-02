package fix

import scala.annotation.targetName

trait Q { def m: String = "Q"; @targetName("tn") def n: Int = 1 }
trait R { def m: String = "R"; @targetName("tn") def n: Int = 2 }
trait P extends Q with R {
  override def m: String = "P" + new Object { override def toString = P.super[Q].m + P.super[R].m }
  @targetName("tn") override def n: Int = 3 + super.n
}
