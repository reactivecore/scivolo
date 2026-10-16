import scala.concurrent.duration.*

object PackageObjectViews:
  def pair: (Long, TimeUnit) = FiniteDuration(1L, SECONDS)

object Renamed:
  import scala.concurrent.duration.{durationToPair as toPair}
  def pair: (Long, TimeUnit) = toPair(FiniteDuration(2L, SECONDS))
