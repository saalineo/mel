# MEL Hardware Reference — x86_64

Target Architecture: `X86_64`

- Reset Vector: `0x000ffff0`
- Default RAM Base: `0x00100000`
- Default Endianness: `Little`

## Memory Map Layout

| Address Range | Size | Component | Access | Description |
| :--- | :--- | :--- | :--- | :--- |
| `0x000F_0000 - 0x000F_FFFF` | 64 KB | BIOS ROM | R/X | Reset Vector (`0x000F_FFF0`) |
| `0x0010_0000 - 0x040F_FFFF` | 64 MB | Main RAM | R/W/X | System Physical RAM (1MB Base) |
| `0x1000_0000 - 0x1000_00FF` | 256 B | COM1 UART | MMIO R/W | Serial Console |
| `0x2000_0000 - 0x2000_0FFF` | 4 KB | Block Device | MMIO R/W | Virtual Disk Controller |
| `0xFEE0_0000 - 0xFEE0_0FFF` | 4 KB | Local APIC | MMIO R/W | Interrupt Controller |

## ABI Register Conventions

| Register | Role |
| :--- | :--- |
| `rax` | Return Value / Syscall ID |
| `rdi, rsi, rdx, rcx, r8, r9` | Function Arguments |
| `rbx, rsp, rbp, r12..r15` | Callee-saved registers |
| `rsp` | Stack pointer |
