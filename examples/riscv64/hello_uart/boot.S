# MEL — Standalone RISC-V 64-bit Hello UART Boot Payload
#
# Target Architecture: RISC-V 64-bit (RV64GC)
# RAM Base Address   : 0x80000000
# Stack Top Address  : 0x80010000
# UART Base Address  : 0x10000000 (NS16550 THR at offset 0)

.section .text.boot, "ax", @progbits
.globl _start

_start:
    # 1. Initialize Stack Pointer sp = 0x80010000
    lui     sp, 0x80010
    slli    sp, sp, 32
    srli    sp, sp, 32

    # 2. Setup UART Base Address a0 = 0x10000000
    lui     a0, 0x10000

    # 3. Load address of string message into a1
    lla     a1, msg

print_loop:
    # 4. Load next character byte
    lbu     a2, 0(a1)
    # Check for null-terminator (0 byte)
    beqz    a2, halt_loop

    # 5. Write character to UART Transmitter Holding Register (THR)
    sb      a2, 0(a0)

    # 6. Advance message pointer and loop
    addi    a1, a1, 1
    j       print_loop

halt_loop:
    # 7. Low-power wait for interrupt or spin
    wfi
    j       halt_loop

.section .rodata
msg:
    .string "Hello MEL RISC-V!\n"
