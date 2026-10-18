# MEL Power ISA (PowerPC 64) Hello UART Boot Payload

This standalone bare-metal payload demonstrates power-on booting, stack pointer initialization, and character transmission across the NS16550 UART controller on the `powerpc64` architecture.

## Memory & Hardware Map

| Parameter | Value | Description |
| :--- | :--- | :--- |
| **Architecture** | `powerpc64` / `power_iso64` | Power ISA (Big-Endian) |
| **RAM Base Address** | `0x0000_0000` | System physical RAM base |
| **Entry Point PC** | `0x0000_0100` | Power-on reset vector / initial code load address |
| **Initial Stack Pointer** | `0x0001_0000` | Top of initial boot stack (`r1`) |
| **UART MMIO Base** | `0x1000_0000` | NS16550 Transmitter Holding Register (THR) |

## Expected Output
```text
Hello MEL PowerPC!
```

## Running with MEL CLI

```bash
# Execute payload on PowerPC VM
cargo run -p mel-cli -- run examples/powerpc64/hello_uart/boot.bin --arch powerpc64

# Inspect payload disassembly and hex dump
cargo run -p mel-cli -- inspect examples/powerpc64/hello_uart/boot.bin --arch powerpc64

# Debug payload interactively
cargo run -p mel-cli -- debug examples/powerpc64/hello_uart/boot.bin --arch powerpc64
```

## Rebuilding from Source (Optional)

If a PowerPC toolchain (`powerpc-linux-gnu-as` or `clang` / `lld` / `llvm-objcopy`) is available:

```bash
make
# or using clang / llvm:
clang --target=powerpc-linux-gnu -c boot.S -o boot.o
ld.lld -T linker.ld -o boot.elf boot.o
llvm-objcopy -O binary boot.elf boot.bin
```
