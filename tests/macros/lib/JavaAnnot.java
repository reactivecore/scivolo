package macrolib;

import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;

@Retention(RetentionPolicy.RUNTIME)
public @interface JavaAnnot {
    int a();
    String b() default "b";
    int c() default 3;
}
