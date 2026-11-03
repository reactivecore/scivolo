trait PathChunk {
  def segments: Seq[String]
}

object PathChunk {
  implicit class StringPathChunk(s: String) extends PathChunk {
    def segments = Seq(s)
  }
  implicit class SeqPathChunk[T](a: Seq[T])(implicit f: T => PathChunk) extends PathChunk {
    private val rel = a.map(f).foldLeft(RelPath.rel) { case (current, chunk) => current / chunk }
    def segments = rel.segments
  }
}

trait BasePath {
  type ThisType <: BasePath
  def /(chunk: PathChunk): ThisType
}

trait SegmentedPath extends BasePath {
  protected[this] def make(p: Seq[String]): ThisType
  def segments: IndexedSeq[String]
  override def /(chunk: PathChunk): ThisType = make(segments ++ chunk.segments)
}

class RelPath(segments0: Array[String]) extends SegmentedPath {
  val segments: IndexedSeq[String] = segments0.toIndexedSeq
  type ThisType = RelPath
  override protected[this] def make(p: Seq[String]): RelPath = new RelPath(p.toArray[String])
  override def toString = segments.mkString("/")
}

object RelPath {
  val rel: RelPath = new RelPath(Array())
}

@main def forwardaliases(): Unit = {
  import PathChunk._
  println(SeqPathChunk(Seq("a", "b", "c")).segments)
  println(RelPath.rel / "x")
}
