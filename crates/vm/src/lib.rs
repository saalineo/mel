//! # MEL Virtual Machine
//!
//! Virtual motherboard composing CPU, system bus, memory, and MMIO peripherals into a single machine.

pub mod clock;
pub mod factory;
pub mod machine;
pub mod snapshot;

pub use clock::{VirtualClock, DEFAULT_FREQUENCY_HZ};
pub use factory::{
    build_vm_for_arch, build_vm_from_file, build_vm_from_profile, build_vm_from_yaml_str,
};
pub use machine::{
    create_basic_arm64_vm, create_basic_powerpc64_vm, create_basic_riscv64_vm,
    create_basic_x86_64_vm, create_mock_vm, VirtualMachine,
};
pub use mel_core::profile::MachineProfile;
pub use mel_core::{Architecture, MelError, MelResult};
pub use snapshot::{Snapshot, SNAPSHOT_MAGIC, SNAPSHOT_VERSION};
