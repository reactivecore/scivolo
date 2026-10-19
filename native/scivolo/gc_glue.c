// What Scala Native's linker emits for Immix, provided by scivolo at run time instead.

#include <stddef.h>
#include <stdint.h>
#include "nativeThreadTLS.h"
#include "immix/MutatorThread.h"
#include "immix/State.h"

void scalanative_GC_init(void);

SN_ThreadLocal ThreadInfo currentThreadInfo;

int __object_array_id = -1;
int __blob_array_id = -1;
int __array_ids_min = 1;
int __array_ids_max = 0;
int __boxed_ptr_id = -1;
int __weak_ref_ids_min = 1;
int __weak_ref_ids_max = 0;
int __weak_ref_field_offset = -1;

// The VM keeps modules in its permanent heap, a GC root range, so this table stays empty.
void *__modules = NULL;
int __modules_size = 0;

// Roots of each green thread: `frames` points to its frame stack's [low end, top]; [low, high)
// is its native stack while suspended, empty while running.
typedef struct {
    uintptr_t *frames;
    uintptr_t low, high;
} ScivoloThreadRoots;

ScivoloThreadRoots *scivolo_threads = NULL;
int scivolo_threads_count = 0;

typedef struct {
    int object_array, blob_array, arrays_min, arrays_max, boxed_ptr;
    int weak_refs_min, weak_refs_max, weak_referent_offset;
} GcIds;

void scivolo_GC_set_ids(const GcIds *ids) {
    __object_array_id = ids->object_array;
    __blob_array_id = ids->blob_array;
    __array_ids_min = ids->arrays_min;
    __array_ids_max = ids->arrays_max;
    __boxed_ptr_id = ids->boxed_ptr;
    __weak_ref_ids_min = ids->weak_refs_min;
    __weak_ref_ids_max = ids->weak_refs_max;
    __weak_ref_field_offset = ids->weak_referent_offset;
}

void scivolo_GC_set_threads(ScivoloThreadRoots *threads, int count) {
    scivolo_threads = threads;
    scivolo_threads_count = count;
}

// Makes the collector scan the calling code's native stack up to `high`, within [low, high).
void scivolo_GC_switch_stack(void *low, void *high) {
    currentThreadInfo.stackTop = low;
    currentThreadInfo.stackBottom = high;
    currentMutatorThread->stackBottom = (word_t **)high;
}

// Starts the collector on the calling thread, whose stack spans [low, high).
void scivolo_GC_init(void *low, void *high) {
    currentThreadInfo.stackTop = low;
    currentThreadInfo.stackBottom = high;
    currentThreadInfo.isMainThread = true;
    scalanative_GC_init();
}

void StackTrace_PrintStackTrace(void) {}

void scalanative_GC_set_weak_references_collected_callback(void (*callback)(void));

// Set when a collection cleared weak references; the VM runs the program's handler later.
volatile int scivolo_weak_refs_collected = 0;

static void scivolo_note_weak_refs(void) { scivolo_weak_refs_collected = 1; }

void scivolo_GC_watch_weak_refs(void) {
    scalanative_GC_set_weak_references_collected_callback(scivolo_note_weak_refs);
}
