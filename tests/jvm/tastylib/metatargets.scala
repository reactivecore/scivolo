import scala.annotation.meta.{field, param}

class ann extends scala.annotation.StaticAnnotation

class MetaTargets(@(ann @param) @(ann @field) val p: Int, @(ann @field) @(ann @param) val q: Int):
  @(ann @param) @(ann @field) val a: Int = p
