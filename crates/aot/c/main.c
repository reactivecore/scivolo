// Starts the collector and runs the program through the generated `scalai_aot_main`.
#include <stdint.h>
#include <stdio.h>

extern int scalai_aot_main(int argc, char **argv);
extern void *scalai_aot_exception;
// The offsets of a class's name, `String`'s `value` and `count`, and the array header size.
extern const int32_t scalai_aot_layout[4];
void scalai_aot_start_gc(void);

// Prints `java.lang.String` `s` (ASCII only) to stderr.
static void print_string(const char *s) {
    const int32_t *at = scalai_aot_layout;
    const char *chars = *(const char *const *)(s + at[1]);
    int32_t count = *(const int32_t *)(s + at[2]);
    for (int32_t i = 0; i < count; i++)
        fputc(*(const uint16_t *)(chars + at[3] + 2 * i), stderr);
}

int main(int argc, char **argv) {
    scalai_aot_start_gc();
    int code = scalai_aot_main(argc, argv);
    if (scalai_aot_exception != 0) {
        // Temporary until #115 (stack traces): an exception the handler could not report is
        // reported by its class only.
        const char *rtti = *(const char *const *)scalai_aot_exception;
        fprintf(stderr, "Exception in thread \"main\" ");
        print_string(*(const char *const *)(rtti + scalai_aot_layout[0]));
        fputc('\n', stderr);
        return 1;
    }
    return code;
}
