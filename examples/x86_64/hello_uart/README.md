# MEL x86-64 Hello UART Boot Payload

This standalone bare-metal payload demonstrates power-on booting, stack pointer initialization, and character transmission across the NS16550 UART controller on the `x86_64` (AMD64) architecture.

## Memory & Hardware Map

| Parameter | Value | Description |
| :--- | :--- | :--- |
| **Architecture** | `x86_64` / `AMD64` | 64-bit x86 Long Mode |
| **RAM Base Address** | `0x0010_0000` | Code load address & entry point PC (1 MB) |
| **Initial Stack Pointer** | `0x0020_0000` | Top of initial boot stack (2 MB) |
| **UART MMIO Base** | `0x1000_0000` | NS16550 Transmitter Holding Register (THR) |

## Expected Output
```text
Hello MEL x86-64!
```

## Running with MEL CLI

```bash
# Execute payload on x86-64 VM
cargo run -p mel-cli -- run examples/x86_64/hello_uart/boot.bin --arch x86_64

# Inspect payload disassembly and hex dump
cargo run -p mel-cli -- inspect examples/x86_64/hello_uart/boot.bin --arch x86_64

# Debug payload interactively
cargo run -p mel-cli -- debug examples/x86_64/hello_uart/boot.bin --arch x86_64
```

## Rebuilding from Source (Optional)

If standard host or cross x86-64 tools (`as`, `ld`, `objcopy` or `clang`, `lld`, `llvm-objcopy`) are available:

```bash
make
# or using clang / llvm:
clang --target=x86_64-linux-gnu -c boot.S -o boot.o
ld.lld -T linker.ld -o boot.elf boot.o
llvm-objcopy -O binary boot.elf boot.bin
```
