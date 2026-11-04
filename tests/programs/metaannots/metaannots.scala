import scala.annotation.meta.{field, getter, setter}

final class Cell(@(inline @getter @setter) private var _key: Int, @inline private var _plain: Int) {
  @inline var hinted: Int = 0
  @(inline @getter) val read: Int = 1
  @(noinline @field) var stored: Int = 2
  def key: Int = _key
  def bump(): Unit = { _key += 1; _plain += 1; hinted += 1 }
  def sum: Int = _key + _plain + hinted + read + stored
}

object metaannots {
  def main(args: Array[String]): Unit = {
    val c = new Cell(1, 2)
    c.bump()
    println(c.sum)
  }
}
