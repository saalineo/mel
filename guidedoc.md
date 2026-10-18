# MEL — Comprehensive System & Developer Guide

> **MEL** is a bare-metal systems-engineering simulation for developers and systems programmers.
> Players start with a virtual machine built around real CPU architectures and progressively build, configure, understand, and replace the software stack required to make the machine useful.

---

## Table of Contents

1. [Architecture Philosophy & Product Principles](#1-architecture-philosophy--product-principles)
2. [Workspace Architecture & Crate Graph](#2-workspace-architecture--crate-graph)
3. [Supported CPU Architectures & Memory Maps](#3-supported-cpu-architectures--memory-maps)
4. [Hardware Peripheral Subsystems](#4-hardware-peripheral-subsystems)
5. [Virtual Motherboard & Execution Engine (`mel-vm`)](#5-virtual-motherboard--execution-engine-mel-vm)
6. [Interactive Debugger & Disassembler (`mel-debugger`)](#6-interactive-debugger--disassembler-mel-debugger)
7. [Storage Subsystem & Filesystem Tooling (`mel-storage`)](#7-storage-subsystem--filesystem-tooling-mel-storage)
8. [CLI User Guide & Interactive TUI Visualizer (`mel-cli`)](#8-cli-user-guide--interactive-tui-visualizer-mel-cli)
9. [Standalone Bare-Metal Assembly Samples & Custom Payloads](#9-standalone-bare-metal-assembly-samples)
10. [Developer Guide, Testing Matrix & Verification](#10-developer-guide-testing-matrix--verification)
11. [Roadmap & Milestone Progression](#11-roadmap--milestone-progression)

---

## 1. Architecture Philosophy & Product Principles

### 1.1 Core Principle
> **"Do not simulate the appearance of systems engineering. Simulate the systems."**

In MEL:
- If the developer writes machine code, it must execute on real CPU emulator backends.
- If a bootloader is written, the machine must physically boot through its instructions.
- If stack or memory initialization is broken, the machine will crash or trigger architectural traps.
- If a kernel, scheduler, or MMU page table is configured, the virtual motherboard enforces architectural invariants.

### 1.2 Technology Reuse vs. Custom Architecture
MEL leverages mature, battle-tested open-source libraries where appropriate, while focusing core development on the simulation fabric:

| Subsystem | Underlying Technology | MEL Custom Layer |
| :--- | :--- | :--- |
| **CPU Emulation** | `unicorn-engine` 2.1 | `mel-cpu`: Generic `CpuBackend` trait, register abstraction, MMIO bus hooking, Port I/O dispatching, trap handling |
| **Disassembly** | `capstone` 0.12 | `mel-debugger`: Multi-architecture instruction decoding, formatted dumps, symbol resolution |
| **Filesystems** | `fatfs` 0.3.6 | `mel-storage`: Copy-on-Write RAM overlays, transactional disk backing, automated image formatting |
| **CLI & Parsing** | `clap` 4.5, `serde` 1.0 | `mel-cli`, `mel-core`: Machine profiles, YAML configuration schemas, interactive REPL |
| **Motherboard & Bus** | Pure Rust (`mel-bus`, `mel-vm`) | Priority device routing, RAM management, deterministic cycle clock, polymorphic snapshotting |

---

## 2. Workspace Architecture & Crate Graph

The MEL project is organized as a Cargo workspace containing 11 specialized crates:

```text
                                  ┌─────────────┐
                                  │   mel-cli   │
                                  └──────┬──────┘
                                         │
                 ┌───────────────────────┼───────────────────────┐
                 │                       │                       │
                 ▼                       ▼                       ▼
          ┌─────────────┐         ┌─────────────┐         ┌─────────────┐
          │   mel-vm    │◄────────┤mel-debugger │         │mel-scenarios│
          └──────┬──────┘         └──────┬──────┘         └──────┬──────┘
                 │                       │                       │
       ┌─────────┴─────────┬─────────────┴─────────┐             │
       ▼                   ▼                       ▼             ▼
┌─────────────┐     ┌─────────────┐         ┌─────────────┐┌─────────────┐
│  mel-loader │     │ mel-devices │         │   mel-cpu   ││ mel-storage │
└──────┬──────┘     └──────┬──────┘         └──────┬──────┘└──────┬──────┘
       │                   │                       │              │
       └───────────┬───────┴───────────────┬───────┴──────────────┘
                   ▼                       ▼
            ┌─────────────┐         ┌─────────────┐
            │   mel-bus   │◄────────┤ mel-memory  │
            └──────┬──────┘         └──────┬──────┘
                   │                       │
                   └───────────┬───────────┘
                               ▼
                        ┌─────────────┐
                        │  mel-core   │
                        └─────────────┘
```

### 2.1 Crate Breakdown

1. [`crates/core`](file:///home/oxide/Desktop/mel/crates/core/src/lib.rs): Fundamental types (`Address`, `Size`, `RegisterId`), architectural enumerations (`Architecture`, `Endianness`), standard memory maps (`ArchitectureMemoryMap`), and YAML profile models (`MachineProfile`).
2. [`crates/memory`](file:///home/oxide/Desktop/mel/crates/memory/src/lib.rs): System physical RAM backing, memory region permissions (`Read`, `Write`, `Execute`), and bounds checking.
3. [`crates/bus`](file:///home/oxide/Desktop/mel/crates/bus/src/lib.rs): Unified `SystemBus` interconnect routing reads and writes to RAM or mapped `BusDevice` MMIO peripherals.
4. [`crates/cpu`](file:///home/oxide/Desktop/mel/crates/cpu/src/lib.rs): CPU execution backends implementing the `CpuBackend` trait for RISC-V 64, ARM64, x86-64, Power ISA 64, and Mock testing CPUs.
5. [`crates/devices`](file:///home/oxide/Desktop/mel/crates/devices/src/lib.rs): Hardware MMIO peripherals: NS16550 UART, SiFive CLINT, SiFive PLIC, Boot ROM, and Virtual Block Device.
6. [`crates/storage`](file:///home/oxide/Desktop/mel/crates/storage/src/lib.rs): `DiskImage` virtual block storage backing, Copy-on-Write overlay engine, and `FatFs` filesystem manipulation.
7. [`crates/loader`](file:///home/oxide/Desktop/mel/crates/loader/src/lib.rs): Raw binary payload loading into guest address space.
8. [`crates/debugger`](file:///home/oxide/Desktop/mel/crates/debugger/src/lib.rs): Multi-arch Capstone disassembler, breakpoint/watchpoint management, register inspection, and interactive REPL.
9. [`crates/scenarios`](file:///home/oxide/Desktop/mel/crates/scenarios/src/lib.rs): Scenario runner framework and grading assertions (Phase 3).
10. [`crates/vm`](file:///home/oxide/Desktop/mel/crates/vm/src/lib.rs): `VirtualMachine` motherboard composing CPU, Bus, RAM, Peripherals, `VirtualClock`, and Version 2 Polymorphic Snapshots.
11. [`crates/cli`](file:///home/oxide/Desktop/mel/crates/cli/src/lib.rs): User command-line interface providing `run`, `debug`, `inspect`, and `machine` subcommands.

---

## 3. Supported CPU Architectures & Memory Maps

MEL officially supports **4 production-grade CPU architectures**:

```text
┌───────────────────────────┬───────────────────────────┐
│     RISC-V (RV64GC)       │      ARM64 (AArch64)      │
│  - Little-Endian          │  - Little-Endian          │
│  - RAM Base: 0x8000_0000  │  - RAM Base: 0x4000_0000  │
│  - Reset: 0x8000_0000     │  - Reset: 0x4000_0000     │
│  - UART: 0x1000_0000      │  - UART: 0x0900_0000      │
├───────────────────────────┼───────────────────────────┤
│       x86-64 (AMD64)      │     Power ISA (PPC64)     │
│  - Little-Endian          │  - Big-Endian             │
│  - RAM Base: 0x0010_0000  │  - RAM Base: 0x0000_0000  │
│  - Reset: 0x0010_0000     │  - Reset: 0x0000_0100     │
│  - UART: 0x1000_0000      │  - UART: 0x1000_0000      │
└───────────────────────────┴───────────────────────────┘
```

### 3.1 Memory Map Tables

#### A. RISC-V 64 (`riscv64`)
| Address Range | Size | Peripheral / Region | Description |
| :--- | :--- | :--- | :--- |
| `0x0000_1000` - `0x0000_1FFF` | 4 KB | Boot ROM | Hardware reset vector & jump instruction |
| `0x0200_0000` - `0x0200_FFFF` | 64 KB | CLINT | Core Local Interruptor (`mtime`, `mtimecmp`, `msip`) |
| `0x0C00_0000` - `0x0FFF_FFFF` | 64 MB | PLIC | Platform-Level Interrupt Controller |
| `0x1000_0000` - `0x1000_00FF` | 256 B | UART 16550 | Serial console MMIO |
| `0x2000_0000` - `0x2000_0FFF` | 4 KB | Block Storage | Virtual disk controller |
| `0x8000_0000` - `0x83FF_FFFF` | 64 MB | Main RAM | Primary physical system memory |

#### B. ARM64 (`arm64`)
| Address Range | Size | Peripheral / Region | Description |
| :--- | :--- | :--- | :--- |
| `0x0000_0000` - `0x0000_FFFF` | 64 KB | Boot ROM | ROM reset vector |
| `0x0800_0000` - `0x0800_FFFF` | 64 KB | GIC | Generic Interrupt Controller |
| `0x0900_0000` - `0x0900_00FF` | 256 B | UART 16550 | Serial console MMIO (QEMU virt standard) |
| `0x2000_0000` - `0x2000_0FFF` | 4 KB | Block Storage | Virtual disk controller |
| `0x4000_0000` - `0x43FF_FFFF` | 64 MB | Main RAM | Primary physical system memory |

#### C. x86-64 (`x86_64`)
| Address Range | Size | Peripheral / Region | Description |
| :--- | :--- | :--- | :--- |
| `0x000F_0000` - `0x000F_FFFF` | 64 KB | BIOS / Boot ROM | Reset vector at `0x000F_FFF0` |
| `0x0010_0000` - `0x040F_FFFF` | 64 MB | Main RAM | 1 MB base extended memory |
| `0x1000_0000` - `0x1000_00FF` | 256 B | UART MMIO / COM1 | Port `0x3F8` and MMIO mirror |
| `0x2000_0000` - `0x2000_0FFF` | 4 KB | Block Storage | Virtual disk controller |
| `0xFEE0_0000` - `0xFEE0_0FFF` | 4 KB | Local APIC | Advanced Programmable Interrupt Controller |

#### D. Power ISA (`powerpc64`)
| Address Range | Size | Peripheral / Region | Description |
| :--- | :--- | :--- | :--- |
| `0x0000_0000` - `0x03FF_FFFF` | 64 MB | Main RAM | Interrupt reset vectors at `0x0000_0100` |
| `0x1000_0000` - `0x1000_00FF` | 256 B | UART 16550 | Serial console MMIO |
| `0x2000_0000` - `0x2000_0FFF` | 4 KB | Block Storage | Virtual disk controller |
| `0x3000_0000` - `0x3000_FFFF` | 64 KB | OpenPIC / XIVE | Power ISA Interrupt Controller |

---

## 4. Hardware Peripheral Subsystems

### 4.1 NS16550 UART Controller
The UART peripheral emulates a standard NS16550 device:
- **Base Offset `0x00`**: Transmitter Holding Register (THR, write) / Receiver Buffer Register (RBR, read) / Divisor Latch LSB (DLL, when DLAB=1).
- **Base Offset `0x01`**: Interrupt Enable Register (IER) / Divisor Latch MSB (DLM, when DLAB=1).
- **Base Offset `0x02`**: Interrupt Identification Register (IIR) / FIFO Control Register (FCR).
- **Base Offset `0x03`**: Line Control Register (LCR, bit 7 toggles DLAB).
- **Base Offset `0x04`**: Modem Control Register (MCR).
- **Base Offset `0x05`**: Line Status Register (LSR, bit 5 = THRE / Transmitter Empty, bit 0 = DR / Data Ready).

### 4.2 RISC-V CLINT (Core Local Interruptor)
The CLINT provides timer and software interrupt facilities:
- **`0x0000 + 4*hart`**: `msip` (Machine Software Interrupt Pending, bit 0).
- **`0x4000 + 8*hart`**: `mtimecmp` (Machine Timer Compare, generates timer interrupt when `mtime >= mtimecmp`).
- **`0xBFF8`**: `mtime` (64-bit monotonic cycle clock).

### 4.3 RISC-V PLIC (Platform-Level Interrupt Controller)
The PLIC manages external hardware interrupts:
- **`0x0000_0000 + 4*source`**: Interrupt source priority (`1..1023`).
- **`0x0000_1000 + 4*(source/32)`**: Pending bitmask.
- **`0x0000_2000 + 0x80*context`**: Context interrupt enable bitmasks.
- **`0x0020_0000 + 0x1000*context`**: Priority threshold register.
- **`0x0020_0004 + 0x1000*context`**: Claim / Complete handshake register.

### 4.4 Virtual Block Storage Controller
The 4 KB MMIO block device controller provides raw disk sector access:

| Offset | Register | Type | Description |
| :--- | :--- | :--- | :--- |
| `0x00` | `MAGIC` | `u32` (RO) | Magic identifier `0x434F_4C42` (`"BLOC"`) |
| `0x04` | `VERSION` | `u32` (RO) | Controller version (`1`) |
| `0x08` | `CAPACITY` | `u64` (RO) | Disk capacity in 512-byte sectors |
| `0x10` | `SECTOR_NUM` | `u64` (RW) | Target Logical Block Address (LBA) |
| `0x18` | `BUFFER_ADDR` | `u64` (RW) | Guest RAM DMA buffer physical address |
| `0x20` | `COMMAND` | `u32` (RW) | Trigger: `1`=Read, `2`=Write, `3`=Flush |
| `0x24` | `STATUS` | `u32` (RW) | Status: `0`=Ready, `1`=Busy, `2`=Error |
| `0x28` | `SECTOR_SIZE` | `u32` (RO) | Sector size (`512` bytes) |
| `0x2C` | `INT_ENABLE` | `u32` (RW) | Bit 0: Interrupt on completion enable |
| `0x30` | `INT_STATUS` | `u32` (RW) | Bit 0: Interrupt pending (write 1 to clear) |
| `0x100..0x2FF` | `DATA_BUFFER` | `[u8; 512]` | Direct MMIO sector buffer window |

---

## 5. Virtual Motherboard & Execution Engine (`mel-vm`)

### 5.1 VirtualMachine Construction
```rust
use mel_core::Architecture;
use mel_vm::{build_vm_for_arch, create_basic_arm64_vm, create_basic_riscv64_vm, create_basic_x86_64_vm, create_basic_powerpc64_vm};

// Build from standard architectural profile
let mut vm = build_vm_for_arch(Architecture::Arm64)?;

// Or use convenience constructors
let mut riscv_vm = create_basic_riscv64_vm()?;
let mut arm_vm = create_basic_arm64_vm()?;
let mut x86_vm = create_basic_x86_64_vm()?;
let mut ppc_vm = create_basic_powerpc64_vm()?;
```

### 5.2 Deterministic VirtualClock
`VirtualClock` tracks simulation execution independently of host clock jitter:
- Default frequency: `10_000_000 Hz` (10 MHz).
- Step synchronization: Every `vm.step()` advances the clock and dispatches peripheral ticks (e.g. incrementing CLINT `mtime`).

### 5.3 Version 2 Polymorphic Snapshots
Snapshots save and restore complete VM state across all 4 architectures:
```rust
// Save state to in-memory Snapshot
let snapshot = vm.save_snapshot()?;

// Persist to compressed .mels file on disk
vm.save_snapshot_to_file("checkpoint.mels")?;

// Restore state
let mut restored_vm = build_vm_for_arch(Architecture::Riscv64)?;
restored_vm.load_snapshot_from_file("checkpoint.mels")?;
```

---

## 6. Interactive Debugger & Disassembler (`mel-debugger`)

### 6.1 Multi-Architecture Disassembly
`mel-debugger::disassemble` parses raw byte slices into structured assembly instructions:
```rust
use mel_core::Architecture;
use mel_debugger::disassemble;

let code = [0x37, 0x05, 0x00, 0x10]; // lui a0, 0x10000
let insns = disassemble(Architecture::Riscv64, &code, 0x8000_0000)?;
for insn in insns {
    println!("{}", insn.format_line());
}
```

### 6.2 Debugger Command Reference

| Command | Aliases | Syntax | Description |
| :--- | :--- | :--- | :--- |
| `step` | `s` | `step [n]` | Execute 1 or `n` CPU instructions |
| `continue` | `c`, `cont` | `continue` | Run execution until breakpoint, trap, or halt |
| `regs` | `r`, `reg` | `regs` | Display all architectural registers |
| `mem` | `m`, `x` | `mem <addr> [len]` | Display hex dump of physical memory |
| `disasm` | `dis`, `d` | `disasm [addr] [count]` | Disassemble `count` instructions |
| `break` | `b`, `bp` | `break <addr>` | Set permanent execution breakpoint |
| `tbreak` | `tb` | `tbreak <addr>` | Set one-shot temporary breakpoint |
| `watch` | `w` | `watch <addr> [size] [kind]` | Set memory watchpoint (`r`, `w`, `rw`) |
| `del` | `delete`, `rm` | `del <id>` | Delete breakpoint or watchpoint by ID |
| `info break` | `bl` | `info break` | List all breakpoints and watchpoints |
| `reset` | - | `reset` | Reset VM to power-on reset state |
| `help` | `h`, `?` | `help` | Display debugger command summary |
| `quit` | `q`, `exit` | `quit` | Exit debugger session |

---

## 7. Storage Subsystem & Filesystem Tooling (`mel-storage`)

### 7.1 Copy-on-Write Disk Backing
`mel-storage::DiskImage` enables safe, isolated disk modifications:
```rust
use mel_storage::DiskImage;

// Open disk with Copy-on-Write overlay enabled
let mut disk = DiskImage::open_with_cow("rootfs.img")?;

// Writes modify only the RAM overlay
disk.write_sector(10, &[0xAA; 512])?;

// Commit changes back to disk, or rollback
disk.commit_cow_overlay()?;
// or: disk.rollback_cow_overlay();
```

### 7.2 FAT Filesystem Injection
`mel-storage::FatFs` allows injecting files into guest FAT images without mounting:
```rust
use mel_storage::FatFs;

let mut fat = FatFs::format_disk(&mut disk, 512 * 1024 * 1024)?;
fat.create_directory("boot")?;
fat.write_file("boot/kernel.bin", &kernel_bytes)?;
```

---

## 8. CLI User Guide & Interactive TUI Visualizer (`mel-cli`)

### 8.1 Subcommands Overview

| Command | Syntax | Description |
| :--- | :--- | :--- |
| **`mel run`** | `mel run <FILE> [-a <ARCH>] [-c <CYCLES>] [-e <ADDR>] [-q]` | Execute binary headless and output UART log |
| **`mel inspect`** | `mel inspect <FILE> [-a <ARCH>] [-n <COUNT>]` | Inspect binary size, Capstone disassembly preview, and raw hex dump |
| **`mel debug`** | `mel debug <FILE> [-a <ARCH>] [-e <ADDR>] [--tui]` | Launch CLI line-based REPL or full interactive 4-quadrant TUI |
| **`mel machine`** | `mel machine [list \| info -a <ARCH>]` | Query supported architectures, reset vectors, and memory maps |

---

### 8.2 Launching the Interactive TUI Visualizer (`--tui`)

The TUI provides a live, 4-quadrant diagnostic dashboard with real-time CPU register diff tracking, disassembly centering on the active Program Counter (PC), a hexadecimal memory dump viewer, a bidirectional ANSI UART terminal, and an interactive command bar.

```bash
# RISC-V 64 (Default)
cargo run -p mel-cli -- debug <FILE> --tui

# ARM64 (AArch64)
cargo run -p mel-cli -- debug <FILE> -a arm64 --tui

# x86-64 (specify entry point if executing in RAM)
cargo run -p mel-cli -- debug <FILE> -a x86_64 -e 0x100000 --tui

# Power ISA 64 (PowerPC)
cargo run -p mel-cli -- debug <FILE> -a powerpc64 --tui
```

#### TUI Dashboard Layout
```text
┌─ [1] CPU Registers & Status Flags ────────┬─ [2] Disassembly (PC) ──────────────────┐
│ PC:  0x0000000080000000  SP: 0x...        │ => 0x80000000: 93 00 a0 00  addi x1, x0, 10
│ Status: [MIE: 1, MPIE: 0, MPP: M]         │    0x80000004: 13 01 40 01  addi x2, x0, 20
│ x1 (ra):  0x000000000000000a  (*)         │    0x80000008: b3 81 20 00  add  x3, x1, x2
│ x2 (sp):  0x0000000000000014  (*)         │ ●  0x8000000c: 73 00 10 00  ebreak
├─ [3] Memory Hex Viewer ───────────────────┼─ [4] UART Interactive Terminal ─────────┤
│ Address       00 01 02 03 04 05 06 07 ... │ Booting kernel...                       │
│ 0x80000000    93 00 a0 00 13 01 40 01 ... │ [MEL] Ready>                            │
├───────────────────────────────────────────┴─────────────────────────────────────────┤
│ [5] (dbg)> step 1█                                                                  │
│ [Step] Stepped to PC: 0x80000004  |  F5 Cont | F10 Step | F9 Bp | Tab Pane | ESC Pause
└─────────────────────────────────────────────────────────────────────────────────────┘
```

#### Hotkeys & Navigation

| Key | Action |
| :--- | :--- |
| **`F10`** | **Single Step**: Execute 1 instruction and refresh all registers, disassembly, memory, and UART outputs. |
| **`F5`** | **Continue**: Resume continuous non-blocking background VM execution. |
| **`F9`** | **Toggle Breakpoint**: Place or remove a breakpoint at the current PC address. |
| **`Esc`** | **Pause / Clear**: Pause running execution, clear current command prompt, or focus the command bar. |
| **`Tab` / `BackTab`** | **Cycle Panes**: Switch active focus between `[1] Registers` $\leftrightarrow$ `[2] Disassembly` $\leftrightarrow$ `[3] Memory` $\leftrightarrow$ `[4] UART` $\leftrightarrow$ `[5] Command Bar`. |
| **`Ctrl+C` / `Ctrl+Q`** | **Quit**: Terminate the session and restore terminal state. |

#### Interactive REPL Command Bar `(dbg)>`

When the Command Bar `[5]` is focused, you can enter any debugger command:
- `step [n]` (alias: `s`) — Single-step 1 or $n$ instructions.
- `cont` (alias: `c`, `continue`) — Run until breakpoint, trap, or halt.
- `break <addr|symbol>` (alias: `b`) — Set permanent execution breakpoint.
- `tbreak <addr|symbol>` (alias: `tb`) — Set temporary one-shot breakpoint.
- `watch <addr> [size] [r|w|rw]` (alias: `w`) — Set memory watchpoint.
- `del <id>` (alias: `rm`) — Delete breakpoint or watchpoint by ID.
- `info break` (alias: `bl`) — List registered breakpoints and watchpoints.
- `mem <addr> [len]` (alias: `m`, `x`) — Jump memory hex viewer to address.
- `disasm [addr] [count]` (alias: `d`) — Jump disassembly viewer.
- `regs` (alias: `r`) — Refresh register panel.
- `reset` — Reset CPU and all peripherals to power-on reset state.
- `help` (alias: `?`) — Show command reference summary.
- `quit` (alias: `q`, `exit`) — Exit debugger.

---

### 8.3 Beyond `Hello MEL`: Simulation Capabilities & OS Progression

While `Hello MEL` demonstrates initial UART output and stack initialization, **MEL is a bare-metal simulation and operating system progression platform** capable of running arbitrary machine code, device drivers, and OS kernels:

1. **Custom Computation & Algorithms**:
   - Run arbitrary bare-metal code: sorting, cryptography, mathematics, and data structures.
   - Trace register changes, memory accesses, and instruction pipelines step-by-step.
2. **Two-Way Interactive UART Applications**:
   - The UART peripheral is bidirectional. Keystrokes typed into pane `[4]` are pushed to the UART RX FIFO and read by the guest software (`0x1000_0000` / `0x0900_0000`), allowing interactive guest command shells.
3. **Hardware Timers & Preemptive Multitasking (CLINT)**:
   - Configure the Core Local Interruptor (CLINT) timer (`mtime` / `mtimecmp`).
   - Write Interrupt Service Routines (ISRs) and build preemptive task schedulers.
4. **Platform-Level Interrupt Controller (PLIC)**:
   - Route and prioritize external hardware interrupts from UART and virtual disk devices.
5. **Virtual Block Storage & FAT Filesystem Driver**:
   - Issue sector reads and writes via DMA to the Virtual Block Storage device (`0x2000_0000`).
   - Read and write files to FAT12/FAT16/FAT32 formatted disk images.
6. **Operating System Progression Challenges**:
   - **Bootloader Handoff**: Initializing M-mode, setting up stack/global pointers, and jumping to RAM.
   - **Virtual Memory & MMU (Sv39)**: Building multi-level page tables and enabling address translation.
   - **Cooperative & Preemptive Schedulers**: Saving and restoring thread contexts.
   - **User Mode & System Calls**: Privilege level transitions (M $\rightarrow$ S $\rightarrow$ U) and `ecall` handling.
   - **Interactive Shell & ELF Loader**: Loading executable files from disk into user memory.

---

## 9. Standalone Bare-Metal Assembly Samples

MEL provides reference bare-metal payloads writing `"Hello MEL <ARCH>!\n"` to UART MMIO for all 4 architectures:

### 9.1 RISC-V 64 (`examples/riscv64/hello_uart/`)
- **Source**: [`boot.S`](file:///home/oxide/Desktop/mel/examples/riscv64/hello_uart/boot.S)
- **RAM Base**: `0x8000_0000`, **Initial SP**: `0x8001_0000`, **UART Base**: `0x1000_0000`
```assembly
.section .text.boot, "ax", @progbits
.globl _start

_start:
    lui     sp, 0x80010
    slli    sp, sp, 32
    srli    sp, sp, 32
    lui     a0, 0x10000
    lla     a1, msg

print_loop:
    lbu     a2, 0(a1)
    beqz    a2, halt_loop
    sb      a2, 0(a0)
    addi    a1, a1, 1
    j       print_loop

halt_loop:
    wfi
    j       halt_loop

.section .rodata
msg:
    .string "Hello MEL RISC-V!\n"
```

### 9.2 ARM64 (`examples/arm64/hello_uart/`)
- **Source**: [`boot.S`](file:///home/oxide/Desktop/mel/examples/arm64/hello_uart/boot.S)
- **RAM Base**: `0x4000_0000`, **Initial SP**: `0x4001_0000`, **UART Base**: `0x0900_0000`
```assembly
.section .text.boot, "ax", @progbits
.globl _start

_start:
    movz    x0, #0x4001, lsl #16
    mov     sp, x0
    movz    x1, #0x0900, lsl #16
    adr     x2, msg

print_loop:
    ldrb    w3, [x2], #1
    cbz     w3, halt_loop
    strb    w3, [x1]
    b       print_loop

halt_loop:
    wfe
    b       halt_loop

.section .rodata
msg:
    .asciz "Hello MEL ARM64!\n"
```

### 9.3 x86-64 (`examples/x86_64/hello_uart/`)
- **Source**: [`boot.S`](file:///home/oxide/Desktop/mel/examples/x86_64/hello_uart/boot.S)
- **RAM Base**: `0x0010_0000`, **Initial SP**: `0x0020_0000`, **UART Base**: `0x1000_0000`
```assembly
.code64
.section .text.boot, "ax", @progbits
.globl _start

_start:
    movabsq $0x00200000, %rsp
    movabsq $0x10000000, %rdi
    lea     msg(%rip), %rsi

print_loop:
    movzbq  (%rsi), %rax
    test    %al, %al
    jz      halt_loop
    movb    %al, (%rdi)
    incq    %rsi
    jmp     print_loop

halt_loop:
    pause
    jmp     halt_loop

.section .rodata
msg:
    .asciz "Hello MEL x86-64!\n"
```

### 9.4 Power ISA (`examples/powerpc64/hello_uart/`)
- **Source**: [`boot.S`](file:///home/oxide/Desktop/mel/examples/powerpc64/hello_uart/boot.S)
- **RAM Base**: `0x0000_0000`, **Entry Point**: `0x0000_0100`, **Initial SP**: `0x0001_0000`, **UART Base**: `0x1000_0000`
```assembly
.section .text.boot, "ax", @progbits
.globl _start

_start:
    lis     1, 1
    ori     1, 1, 0
    lis     3, 0x1000
    ori     3, 3, 0
    lis     4, msg@ha
    addi    4, 4, msg@l

print_loop:
    lbz     5, 0(4)
    cmpwi   5, 0
    beq     halt_loop
    stb     5, 0(3)
    addi    4, 4, 1
    b       print_loop

halt_loop:
    b       halt_loop

.section .rodata
msg:
    .string "Hello MEL PowerPC!\n"
```

### 9.5 Compiling & Running Custom Bare-Metal Payloads

You can compile and run your own bare-metal C or Assembly programs on any target architecture.

#### Example: Writing an Interactive Fibonacci / Computation Kernel in RISC-V Assembly
Save as `fibonacci.S`:
```assembly
.section .text.boot, "ax", @progbits
.globl _start

_start:
    # 1. Setup Stack Pointer sp = 0x80010000
    lui     sp, 0x80010
    slli    sp, sp, 32
    srli    sp, sp, 32

    # 2. Compute Fibonacci sequence into RAM buffer at 0x80002000
    lui     t0, 0x80002          # RAM destination buffer
    li      t1, 0                # Fib(0)
    li      t2, 1                # Fib(1)
    li      t3, 10               # Count: 10 iterations

    sd      t1, 0(t0)            # Store Fib(0)
    sd      t2, 8(t0)            # Store Fib(1)
    addi    t0, t0, 16

fib_loop:
    add     t4, t1, t2           # Fib(n) = Fib(n-1) + Fib(n-2)
    sd      t4, 0(t0)            # Store to RAM
    mv      t1, t2
    mv      t2, t4
    addi    t0, t0, 8
    addi    t3, t3, -1
    bnez    t3, fib_loop

    # 3. Halt / Wait for interrupt
    wfi
```

#### Compile and Run:
```bash
# 1. Assemble to ELF with standard GNU or LLVM toolchain
riscv64-unknown-elf-gcc -nostdlib -Ttext 0x80000000 -o fibonacci.elf fibonacci.S

# 2. Extract flat binary payload
riscv64-unknown-elf-objcopy -O binary fibonacci.elf fibonacci.bin

# 3. Launch in the TUI visualizer
cargo run -p mel-cli -- debug fibonacci.bin --tui
```
In the visualizer, press **`F10`** to step instruction-by-instruction, observe registers `t1`..`t4` update live in the **Registers** pane, and view the computed sequence `[0, 1, 1, 2, 3, 5, 8, 13, 21, ...]` populating in the **Memory Viewer** at address `0x80002000`!

---

## 10. Developer Guide, Testing Matrix & Verification

### 10.1 Workspace Verification Commands

```bash
# 1. Typecheck and build all workspace targets
cargo check --workspace --all-targets

# 2. Strict linter audit
cargo clippy --all-targets

# 3. Execute master test suite (all 11 crates)
cargo test --workspace

# 4. Execute 4-architecture matrix boot test
cargo test -p mel-vm --test multi_arch_matrix
```

### 10.2 Assembling New Payloads
Payloads can be compiled using host LLVM/Clang or GNU cross-toolchains:

```bash
# ARM64
clang --target=aarch64-linux-gnu -c boot.S -o boot.o
ld.lld -T linker.ld -o boot.elf boot.o
llvm-objcopy -O binary boot.elf boot.bin

# x86-64
clang --target=x86_64-linux-gnu -c boot.S -o boot.o
ld.lld -T linker.ld -o boot.elf boot.o
llvm-objcopy -O binary boot.elf boot.bin

# Power ISA
clang --target=powerpc-linux-gnu -c boot.S -o boot.o
ld.lld -T linker.ld -o boot.elf boot.o
llvm-objcopy -O binary boot.elf boot.bin
```

---

## 11. Roadmap & Milestone Progression

```text
 Phase 1: Minimal Viable Engine (Tasks 1–20)    ─── [COMPLETED & SIGNED OFF]
 Phase 2: Peripherals & Multi-Arch (Tasks 21–40) ─── [COMPLETED & SIGNED OFF]
 Phase 3: Scenarios, ELF & TUI (Tasks 41–60)     ─── [NEXT]
 Phase 4: OS Progression Lab (Tasks 61–80)       ─── [PLANNED]
```

### Upcoming Phase 3 Deliverables
1. **ELF64 Parser & Segment Loader**: Multi-architecture ELF parsing and virtual memory loading (`mel-loader`).
2. **Scenario Execution Engine**: YAML-based challenge definitions, grading assertions, and automated harnesses (`mel-scenarios`).
3. **Ratatui Terminal User Interface**: Multi-window interactive dashboard featuring live registers, memory hex viewer, disassembly pane, and interactive VTE UART terminal (`mel-cli`).
4. **PyO3 Python Bindings**: Native bindings and Python test runner harness (`mel-py`).
