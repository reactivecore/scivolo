// What Scala Native's linker emits for Immix, provided by scalai at run time instead.

#include <stddef.h>
#include <stdint.h>
#include "nativeThreadTLS.h"

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

// Bounds of the interpreter's frame stack: [0] its low end, [1] the current top.
uintptr_t *scalai_frame_stack = NULL;

typedef struct {
    int object_array, blob_array, arrays_min, arrays_max, boxed_ptr;
    int weak_refs_min, weak_refs_max, weak_referent_offset;
} GcIds;

void scalai_GC_set_ids(const GcIds *ids) {
    __object_array_id = ids->object_array;
    __blob_array_id = ids->blob_array;
    __array_ids_min = ids->arrays_min;
    __array_ids_max = ids->arrays_max;
    __boxed_ptr_id = ids->boxed_ptr;
    __weak_ref_ids_min = ids->weak_refs_min;
    __weak_ref_ids_max = ids->weak_refs_max;
    __weak_ref_field_offset = ids->weak_referent_offset;
}

void scalai_GC_set_frame_stack(uintptr_t *bounds) { scalai_frame_stack = bounds; }

// Starts the collector on the calling thread, whose stack spans [low, high).
void scalai_GC_init(void *low, void *high) {
    currentThreadInfo.stackTop = low;
    currentThreadInfo.stackBottom = high;
    currentThreadInfo.isMainThread = true;
    scalanative_GC_init();
}

void StackTrace_PrintStackTrace(void) {}

void scalanative_GC_set_weak_references_collected_callback(void (*callback)(void));

// Set when a collection cleared weak references; the VM runs the program's handler later.
volatile int scalai_weak_refs_collected = 0;

static void scalai_note_weak_refs(void) { scalai_weak_refs_collected = 1; }

void scalai_GC_watch_weak_refs(void) {
    scalanative_GC_set_weak_references_collected_callback(scalai_note_weak_refs);
}
