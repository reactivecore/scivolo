package paths

import scala.language.implicitConversions

class Chunk(val s: String)

trait ChunkConversion:
  implicit def fromString(s: String): Chunk = Chunk(s + "!")

object Chunk extends ChunkConversion:
  private[Chunk] implicit def internal(s: String): Chunk = Chunk(s)

trait Box { def v: Int }

object Box:
  private implicit object secret extends Box { def v = 1 }
  implicit val shown: Box = new Box { def v = 2 }

object Use:
  def chunk(c: Chunk): String = c.s
  val a = chunk("a")
  val b = summon[Box].v
