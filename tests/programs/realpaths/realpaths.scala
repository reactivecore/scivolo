import java.nio.file.{LinkOption, Path, Paths}
import scala.util.Try

object Links {
  def real(src: Path): Option[Path] = Try(src.toRealPath()).toOption
  def noFollow(src: Path): Path = src.toRealPath(LinkOption.NOFOLLOW_LINKS)
}

@main def realpaths(): Unit = {
  println(Links.real(Paths.get("/")))
  println(Links.noFollow(Paths.get("/")))
  println(Links.real(Paths.get("/no/such/file")))
}
