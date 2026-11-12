package units
package text

import units.Labels

final class Line private (prefix: String) extends Shown(prefix) {
  def this() = this(null)
}

abstract class Shown(private var prefix: String) {
  self: Line =>

  if (prefix == null) prefix = Labels.prefix

  def of(v: Int): String = prefix + v
}
