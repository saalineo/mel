# MEL Hardware Reference — riscv64

Target Architecture: `Riscv64`

- Reset Vector: `0x80000000`
- Default RAM Base: `0x80000000`
- Default Endianness: `Little`

## Memory Map Layout

| Address Range | Size | Component | Access | Description |
| :--- | :--- | :--- | :--- | :--- |
| `0x0000_1000 - 0x0000_1FFF` | 4 KB | Boot ROM | R/X | Reset vector & jump payload |
| `0x0200_0000 - 0x0200_FFFF` | 64 KB | CLINT | MMIO R/W | Timer & software interrupts |
| `0x0C00_0000 - 0x0FFF_FFFF` | 64 MB | PLIC | MMIO R/W | Platform-level interrupt controller |
| `0x1000_0000 - 0x1000_00FF` | 256 B | UART 16550 | MMIO R/W | Serial terminal console |
| `0x2000_0000 - 0x2000_0FFF` | 4 KB | Block Device | MMIO R/W | Virtual disk storage controller |
| `0x8000_0000 - 0x83FF_FFFF` | 64 MB | Main RAM | R/W/X | System physical RAM |

## CLINT Register Offsets

| Offset | Register | Description |
| :--- | :--- | :--- |
| `0x0000` | `msip[0]` | Machine Software Interrupt Pending |
| `0x4000` | `mtimecmp[0]` | Machine Timer Compare (triggers MTIP) |
| `0xBFF8` | `mtime` | Machine Monotonic Counter Clock |

## ABI Register Conventions

| Register | ABI Name | Role | Saver |
| :--- | :--- | :--- | :--- |
| `x0` | `zero` | Hardwired Zero | N/A |
| `x1` | `ra` | Return Address | Caller |
| `x2` | `sp` | Stack Pointer | Callee |
| `x8` | `s0`/`fp` | Saved Register / Frame Pointer | Callee |
| `x10..x17` | `a0..a7` | Function Arguments & Return Values | Caller |
