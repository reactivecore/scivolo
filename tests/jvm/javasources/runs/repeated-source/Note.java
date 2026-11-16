import java.lang.annotation.Repeatable;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;

@Repeatable(Notes.class)
@Retention(RetentionPolicy.SOURCE)
public @interface Note {
  String value();
}
