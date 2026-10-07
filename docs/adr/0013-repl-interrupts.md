# ADR 0013 — Ctrl-C stops the running REPL input

Status: accepted · Date: 2026-10-07

## Context
In the REPL, Ctrl-C during an evaluation ended the process. An input that loops or sleeps must
stop and leave the session usable. Green threads run on one OS thread, and the interpreter
already counts calls and loop back-edges to preempt (ADR 0003).

## Decision
- A SIGINT handler (`natives/ffi/sigint.rs`) only increments a process-wide counter; it is
  installed without `SA_RESTART` once the REPL asks to watch interrupts. Scripts keep the
  default action.
- The VM takes the counter in the preemption check every `TICKS` ticks and in the scheduler
  loop. While watching, the idle scheduler waits in slices of 100 ms.
- The first SIGINT of an input marks the main green thread to stop and wakes it from `sleep`,
  `park` or `wait` (the native wake of an interrupt, without setting `Thread.interrupted`, which
  would outlive the input). The next tick of the main thread then throws `java.lang.ThreadDeath`,
  which the REPL prints like any uncaught exception.
- A second SIGINT before the input ends exits the process with 130, as dotty's `ReplDriver`
  does on a second Ctrl-C.

## Consequences
- A `ThreadDeath` thrown at a loop back-edge leaves its method without running that method's
  own handlers; callers' handlers and `finally` blocks run. A thread that ends releases the
  monitors it still holds.
- A main thread blocked on a contended monitor or in blocking C I/O stops only after the wait,
  one inside a C callback only after the callback returns.
- During a long CPU-bound C call no SIGINT is polled, not even the second, until the call returns.
- Background green threads keep running.
