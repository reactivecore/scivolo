// Stack overflow: the limit compiled functions compare their stack pointer with on entry, and a
// SIGSEGV handler on an alternate stack for an overflow past it, which stops the program.
// Temporary until #115 (threads): only the main thread's stack is guarded.
#include <signal.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define MARGIN (512 * 1024)
#define RESERVE (128 * 1024)
#define GUARD (1024 * 1024)
#define ALTERNATE (64 * 1024)

// Below this stack pointer a compiled function throws StackOverflowError.
char *scalai_aot_stack_limit = 0;
// The main thread's stack: its lowest address and the address past its end.
char *scalai_aot_stack_low = 0, *scalai_aot_stack_high = 0;
static char *saved_limit = 0;
static int overflowing = 0;
static struct sigaction previous_segv, previous_bus;

static void fatal(const char *text) {
    write(2, text, strlen(text));
    _exit(1);
}

static void on_fault(int sig, siginfo_t *info, void *context) {
    char *addr = info->si_addr, *low = scalai_aot_stack_low;
    if (addr >= low - GUARD && addr < low + RESERVE)
        fatal("Exception in thread \"main\" java.lang.StackOverflowError\n");
    struct sigaction *previous = sig == SIGSEGV ? &previous_segv : &previous_bus;
    if (previous->sa_flags & SA_SIGINFO) {
        previous->sa_sigaction(sig, info, context);
        return;
    }
    if (previous->sa_handler != SIG_DFL && previous->sa_handler != SIG_IGN) {
        previous->sa_handler(sig);
        return;
    }
    signal(sig, SIG_DFL);
}

// Sets the limit for the stack from `low` to `high` and installs the fault handler.
void scalai_aot_stack_init(char *low, char *high) {
    scalai_aot_stack_low = low;
    scalai_aot_stack_high = high;
    scalai_aot_stack_limit = low + MARGIN;
    size_t size = SIGSTKSZ > ALTERNATE ? SIGSTKSZ : ALTERNATE;
    stack_t ss = {.ss_sp = malloc(size), .ss_size = size, .ss_flags = 0};
    sigaltstack(&ss, 0);
    struct sigaction sa;
    memset(&sa, 0, sizeof sa);
    sa.sa_sigaction = on_fault;
    sa.sa_flags = SA_SIGINFO | SA_ONSTACK;
    sigemptyset(&sa.sa_mask);
    sigaction(SIGSEGV, &sa, &previous_segv);
    sigaction(SIGBUS, &sa, &previous_bus);
}

// Lets the code that constructs a StackOverflowError use the stack's reserve (`open`), or
// restores the limit; an overflow while constructing one stops the program, as in the VM.
void scalai_aot_stack_reserve(int8_t open) {
    if (!open) {
        scalai_aot_stack_limit = saved_limit;
        overflowing = 0;
        return;
    }
    if (overflowing)
        fatal("scalai-aot: stack overflow while throwing StackOverflowError\n");
    overflowing = 1;
    saved_limit = scalai_aot_stack_limit;
    scalai_aot_stack_limit = scalai_aot_stack_low + RESERVE;
}
