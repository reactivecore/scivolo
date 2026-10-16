// Stubs for the runtime operations the spike's generated code calls.
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <math.h>

int64_t rt_exc = 0;
static int64_t modules[4096];

int64_t rt_module(int64_t id) { return (int64_t)&modules[id & 4095]; }
int64_t rt_classalloc(int64_t cls) { return (int64_t)calloc(1, 64); }
int64_t rt_arrayalloc(int64_t ty, int64_t n) { return (int64_t)calloc(1, 16 + 8 * (n > 0 ? n : 0)); }
int64_t rt_method(int64_t obj, int64_t sig) { return 0; }
int64_t rt_dynmethod(int64_t obj, int64_t sig) { return 0; }
int8_t rt_is(int64_t obj, int64_t ty) { return obj != 0; }
int64_t rt_as(int64_t obj, int64_t ty) { return obj; }
int64_t rt_box(int64_t ty, int64_t bits) { return bits; }
int64_t rt_unbox(int64_t ty, int64_t obj) { return obj; }
void rt_throw(int64_t exc) { rt_exc = exc; }
void rt_throw_null(void) { rt_exc = 1; }
void rt_throw_bounds(int64_t idx, int64_t len) { rt_exc = 2; }
void rt_throw_div(void) { rt_exc = 3; }
void rt_unreachable(void) { fprintf(stderr, "unreachable\n"); abort(); }
int64_t rt_stackalloc(int64_t bytes) { return (int64_t)calloc(1, bytes); }
double rt_frem(double a, double b) { return fmod(a, b); }
float rt_fremf(float a, float b) { return fmodf(a, b); }
