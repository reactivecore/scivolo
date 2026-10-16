// Stack overflow: the limit compiled functions compare their stack pointer with on entry, and a
// SIGSEGV handler on an alternate stack for an overflow past it, which stops the program.
#include <signal.h>
#include <stdint.h>
#include <string.h>
#include <unistd.h>

#define MARGIN (512 * 1024)
#define RESERVE (128 * 1024)
#define GUARD (1024 * 1024)

// Below this stack pointer a compiled function throws StackOverflowError.
char *scalai_aot_stack_limit = 0;
static char *stack_low = 0;
static struct sigaction previous_segv, previous_bus;

static void on_fault(int sig, siginfo_t *info, void *context) {
    char *addr = info->si_addr;
    if (addr >= stack_low - GUARD && addr < stack_low + RESERVE) {
        static const char text[] = "Exception in thread \"main\" java.lang.StackOverflowError\n";
        write(2, text, sizeof text - 1);
        _exit(1);
    }
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

// Sets the limit for the stack whose lowest address is `low` and installs the fault handler.
void scalai_aot_stack_init(char *low) {
    stack_low = low;
    scalai_aot_stack_limit = low + MARGIN;
    static char alternate[64 * 1024];
    stack_t ss = {.ss_sp = alternate, .ss_size = sizeof alternate, .ss_flags = 0};
    sigaltstack(&ss, 0);
    struct sigaction sa;
    memset(&sa, 0, sizeof sa);
    sa.sa_sigaction = on_fault;
    sa.sa_flags = SA_SIGINFO | SA_ONSTACK;
    sigemptyset(&sa.sa_mask);
    sigaction(SIGSEGV, &sa, &previous_segv);
    sigaction(SIGBUS, &sa, &previous_bus);
}

// Lets the code that constructs a StackOverflowError use the stack's reserve (`open`), or ends
// that.
void scalai_aot_stack_reserve(int8_t open) {
    scalai_aot_stack_limit = stack_low + (open ? RESERVE : MARGIN);
}
