// Starts the collector and runs the program through the generated `scalai_aot_main`.
#include <stdio.h>

extern int scalai_aot_main(int argc, char **argv);
extern void *scalai_aot_exception;
void scalai_aot_start_gc(void *low, void *high);

int main(int argc, char **argv) {
    scalai_aot_start_gc(&argc, __builtin_frame_address(0));
    int code = scalai_aot_main(argc, argv);
    if (scalai_aot_exception != 0) {
        // Temporary until #113 aot 4 (stack traces): the exception is reported without its trace.
        fprintf(stderr, "Exception in thread \"main\" (uncaught)\n");
        return 1;
    }
    return code;
}
