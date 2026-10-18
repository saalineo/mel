#![allow(clippy::useless_conversion)]
#![allow(unsafe_op_in_unsafe_fn)]

use mel_core::RegisterId;
use mel_loader::load_raw_binary;
use mel_vm::{
    VirtualMachine, create_basic_arm64_vm, create_basic_powerpc64_vm, create_basic_riscv64_vm,
    create_basic_x86_64_vm,
};
use pyo3::exceptions::{PyIOError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

#[pyclass(name = "VirtualMachine")]
pub struct PyVirtualMachine {
    vm: VirtualMachine,
}

#[pymethods]
#[allow(clippy::useless_conversion)]
impl PyVirtualMachine {
    #[new]
    #[pyo3(signature = (arch=None, _ram_size=None))]
    pub fn new(arch: Option<&str>, _ram_size: Option<usize>) -> PyResult<Self> {
        let arch_str = arch.unwrap_or("riscv64");

        let vm = match arch_str {
            "riscv64" | "riscv" | "rv64" => create_basic_riscv64_vm()
                .map_err(|e| PyRuntimeError::new_err(format!("Failed to create RISC-V VM: {e}")))?,
            "arm64" | "aarch64" => create_basic_arm64_vm()
                .map_err(|e| PyRuntimeError::new_err(format!("Failed to create ARM64 VM: {e}")))?,
            "x86_64" | "x86-64" | "amd64" => create_basic_x86_64_vm()
                .map_err(|e| PyRuntimeError::new_err(format!("Failed to create x86-64 VM: {e}")))?,
            "powerpc64" | "ppc64" | "powerisa" => create_basic_powerpc64_vm().map_err(|e| {
                PyRuntimeError::new_err(format!("Failed to create PowerISA VM: {e}"))
            })?,
            other => {
                return Err(PyValueError::new_err(format!(
                    "Unsupported architecture: {other}. Use 'riscv64', 'arm64', 'x86_64', or 'powerpc64'"
                )));
            }
        };

        Ok(Self { vm })
    }

    /// Load raw binary into VM memory at specified address and set initial PC.
    pub fn load_binary(&mut self, data: &[u8], addr: u64) -> PyResult<()> {
        load_raw_binary(&mut self.vm, data, addr)
            .map_err(|e| PyIOError::new_err(format!("Failed to load raw binary: {e}")))?;
        Ok(())
    }

    /// Advance simulation by a single instruction step.
    pub fn step(&mut self) -> PyResult<String> {
        let res = self
            .vm
            .step()
            .map_err(|e| PyRuntimeError::new_err(format!("VM step failed: {e}")))?;
        Ok(format!("{res:?}"))
    }

    /// Run simulation for up to max_cycles.
    pub fn run(&mut self, cycles: u64) -> PyResult<String> {
        let res = self
            .vm
            .run(cycles)
            .map_err(|e| PyRuntimeError::new_err(format!("VM run failed: {e}")))?;
        Ok(format!("{res:?}"))
    }

    /// Read architectural register value by name.
    pub fn read_reg(&self, name: &str) -> PyResult<u64> {
        self.vm
            .read_register(RegisterId::named(name))
            .map_err(|e| PyValueError::new_err(format!("Failed to read register '{name}': {e}")))
    }

    /// Write architectural register value by name.
    pub fn write_reg(&mut self, name: &str, val: u64) -> PyResult<()> {
        self.vm
            .write_register(RegisterId::named(name), val)
            .map_err(|e| PyValueError::new_err(format!("Failed to write register '{name}': {e}")))
    }

    /// Read raw memory bytes from VM memory space.
    pub fn read_mem<'py>(
        &self,
        py: Python<'py>,
        addr: u64,
        size: usize,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let mut buf = vec![0u8; size];
        self.vm.read_memory(addr, &mut buf).map_err(|e| {
            PyIOError::new_err(format!("Failed to read memory at 0x{addr:08x}: {e}"))
        })?;
        Ok(PyBytes::new_bound(py, &buf))
    }

    /// Write raw memory bytes into VM memory space.
    pub fn write_mem(&mut self, addr: u64, data: &[u8]) -> PyResult<()> {
        self.vm
            .write_memory(addr, data)
            .map_err(|e| PyIOError::new_err(format!("Failed to write memory at 0x{addr:08x}: {e}")))
    }

    /// Get current UART serial output buffer as bytes.
    pub fn uart_output<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        if let Some(uart) = self.vm.uart() {
            let u = uart
                .lock()
                .map_err(|e| PyRuntimeError::new_err(format!("UART lock poisoned: {e}")))?;
            Ok(PyBytes::new_bound(py, u.peek_output()))
        } else {
            Ok(PyBytes::new_bound(py, &[]))
        }
    }

    /// Get current UART serial output buffer decoded as UTF-8 string.
    pub fn uart_output_string(&self) -> PyResult<String> {
        if let Some(uart) = self.vm.uart() {
            let u = uart
                .lock()
                .map_err(|e| PyRuntimeError::new_err(format!("UART lock poisoned: {e}")))?;
            Ok(String::from_utf8_lossy(u.peek_output()).into_owned())
        } else {
            Ok(String::new())
        }
    }

    /// Reset VM to power-on state.
    pub fn reset(&mut self) -> PyResult<()> {
        self.vm
            .reset()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to reset VM: {e}")))
    }

    /// Read program counter.
    pub fn pc(&self) -> PyResult<u64> {
        self.vm
            .pc()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to get PC: {e}")))
    }

    /// Total accumulated execution cycles.
    pub fn cycle_count(&self) -> u64 {
        self.vm.cycle_count()
    }
}

/// PyO3 extension module definition.
#[pymodule]
fn mel_py(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyVirtualMachine>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_py_virtual_machine_lifecycle() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut py_vm = PyVirtualMachine::new(Some("riscv64"), None).unwrap();
            let pc = py_vm.pc().unwrap();
            assert_eq!(pc, 0x8000_0000);

            py_vm.write_reg("sp", 0x8001_0000).unwrap();
            let sp = py_vm.read_reg("sp").unwrap();
            assert_eq!(sp, 0x8001_0000);

            // NOP instruction (0x00000013 for RISC-V: addi x0, x0, 0)
            let nop = [0x13, 0x00, 0x00, 0x00];
            py_vm.load_binary(&nop, 0x8000_0000).unwrap();

            let step_res = py_vm.step().unwrap();
            assert!(step_res.contains("Ok"));
            assert_eq!(py_vm.pc().unwrap(), 0x8000_0004);

            let mem = py_vm.read_mem(py, 0x8000_0000, 4).unwrap();
            assert_eq!(mem.as_bytes(), &nop);

            let output = py_vm.uart_output_string().unwrap();
            assert_eq!(output, "");
        });
    }
}
