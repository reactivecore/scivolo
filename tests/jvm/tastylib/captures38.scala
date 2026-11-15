import language.experimental.captureChecking
import scala.language.`3.8`

class Label[T]

final class Break[T](val label: Label[T]^{}, val value: T)
