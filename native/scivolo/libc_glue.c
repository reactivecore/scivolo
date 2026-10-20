// `scivolo_`-prefixed exported wrappers of atexit, at_quick_exit and pthread_atfork.

#include <pthread.h>
#include <stdlib.h>

int scivolo_atexit(void (*f)(void)) { return atexit(f); }

int scivolo_at_quick_exit(void (*f)(void)) { return at_quick_exit(f); }

int scivolo_pthread_atfork(void (*prepare)(void), void (*parent)(void), void (*child)(void)) {
    return pthread_atfork(prepare, parent, child);
}
