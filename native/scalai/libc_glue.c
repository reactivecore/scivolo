// `scalai_`-prefixed exported wrappers of atexit, at_quick_exit and pthread_atfork.

#include <pthread.h>
#include <stdlib.h>

int scalai_atexit(void (*f)(void)) { return atexit(f); }

int scalai_at_quick_exit(void (*f)(void)) { return at_quick_exit(f); }

int scalai_pthread_atfork(void (*prepare)(void), void (*parent)(void), void (*child)(void)) {
    return pthread_atfork(prepare, parent, child);
}
