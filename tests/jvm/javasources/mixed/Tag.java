package mixed;

@java.lang.annotation.Retention(java.lang.annotation.RetentionPolicy.RUNTIME)
public @interface Tag {
    String value() default "untagged";
}
