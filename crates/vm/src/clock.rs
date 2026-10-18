//! Deterministic virtual clock and cycle synchronization engine.

use mel_core::{MelError, MelResult};
use serde::{Deserialize, Serialize};

/// Standard default CPU clock frequency: 10 MHz (10,000,000 Hz).
/// At 10 MHz, 1 cycle equals exactly 100 nanoseconds.
pub const DEFAULT_FREQUENCY_HZ: u64 = 10_000_000;

/// Virtual clock maintaining deterministic CPU cycle count and virtual wall-clock time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualClock {
    /// Clock frequency in Hertz (cycles per second).
    frequency_hz: u64,
    /// Total cycles executed since power-on or last reset.
    total_cycles: u64,
}

impl Default for VirtualClock {
    fn default() -> Self {
        Self::new(DEFAULT_FREQUENCY_HZ).expect("default frequency is non-zero")
    }
}

impl VirtualClock {
    /// Construct a new [`VirtualClock`] with the specified frequency in Hertz.
    pub fn new(frequency_hz: u64) -> MelResult<Self> {
        if frequency_hz == 0 {
            return Err(MelError::Config(
                "Clock frequency must be greater than zero".to_string(),
            ));
        }
        Ok(Self {
            frequency_hz,
            total_cycles: 0,
        })
    }

    /// Construct a standard 10 MHz virtual clock.
    pub fn default_10mhz() -> Self {
        Self::default()
    }

    /// Current clock frequency in Hertz.
    pub fn frequency_hz(&self) -> u64 {
        self.frequency_hz
    }

    /// Update clock frequency in Hertz.
    pub fn set_frequency_hz(&mut self, frequency_hz: u64) -> MelResult<()> {
        if frequency_hz == 0 {
            return Err(MelError::Config(
                "Clock frequency must be greater than zero".to_string(),
            ));
        }
        self.frequency_hz = frequency_hz;
        Ok(())
    }

    /// Total number of accumulated cycles executed.
    pub fn total_cycles(&self) -> u64 {
        self.total_cycles
    }

    /// Advance the virtual clock by `cycles` and return the new total cycle count.
    pub fn advance(&mut self, cycles: u64) -> u64 {
        self.total_cycles = self.total_cycles.wrapping_add(cycles);
        self.total_cycles
    }

    /// Reset accumulated cycle count to zero.
    pub fn reset(&mut self) {
        self.total_cycles = 0;
    }

    /// Deterministic virtual wall-clock elapsed time in nanoseconds.
    pub fn wall_time_ns(&self) -> u64 {
        self.cycles_to_ns(self.total_cycles)
    }

    /// Convert a given cycle count to virtual nanoseconds based on current frequency.
    pub fn cycles_to_ns(&self, cycles: u64) -> u64 {
        let total_ns = (cycles as u128)
            .saturating_mul(1_000_000_000)
            .checked_div(self.frequency_hz as u128)
            .unwrap_or(0);
        total_ns as u64
    }

    /// Convert virtual nanoseconds to the corresponding number of cycles.
    pub fn ns_to_cycles(&self, ns: u64) -> u64 {
        let cycles = (ns as u128)
            .saturating_mul(self.frequency_hz as u128)
            .checked_div(1_000_000_000)
            .unwrap_or(0);
        cycles as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtual_clock_initialization_and_advance() {
        let mut clock = VirtualClock::default();
        assert_eq!(clock.frequency_hz(), DEFAULT_FREQUENCY_HZ);
        assert_eq!(clock.total_cycles(), 0);
        assert_eq!(clock.wall_time_ns(), 0);

        let new_cycles = clock.advance(100);
        assert_eq!(new_cycles, 100);
        assert_eq!(clock.total_cycles(), 100);
        // At 10 MHz: 1 cycle = 100 ns -> 100 cycles = 10,000 ns
        assert_eq!(clock.wall_time_ns(), 10_000);

        clock.advance(900);
        assert_eq!(clock.total_cycles(), 1000);
        // 1000 cycles = 100,000 ns = 100 µs
        assert_eq!(clock.wall_time_ns(), 100_000);

        clock.reset();
        assert_eq!(clock.total_cycles(), 0);
        assert_eq!(clock.wall_time_ns(), 0);
    }

    #[test]
    fn test_virtual_clock_custom_frequencies() {
        // 1 GHz (1 cycle = 1 ns)
        let clock_1ghz = VirtualClock::new(1_000_000_000).unwrap();
        assert_eq!(clock_1ghz.cycles_to_ns(500), 500);
        assert_eq!(clock_1ghz.ns_to_cycles(500), 500);

        // 1 MHz (1 cycle = 1000 ns = 1 µs)
        let clock_1mhz = VirtualClock::new(1_000_000).unwrap();
        assert_eq!(clock_1mhz.cycles_to_ns(10), 10_000);
        assert_eq!(clock_1mhz.ns_to_cycles(10_000), 10);

        // Zero frequency rejected
        assert!(VirtualClock::new(0).is_err());
    }

    #[test]
    fn test_virtual_clock_determinism_50k_cycles() {
        let mut clock_a = VirtualClock::default_10mhz();
        let mut clock_b = VirtualClock::default_10mhz();

        const TOTAL_CYCLES: u64 = 50_000;
        const STEP_CHUNK: u64 = 250;

        let mut stepped = 0;
        while stepped < TOTAL_CYCLES {
            clock_a.advance(STEP_CHUNK);
            stepped += STEP_CHUNK;
        }

        // Clock B advances with varying step chunks
        let chunks = [100, 300, 50, 550, 1000];
        let mut stepped_b = 0;
        let mut idx = 0;
        while stepped_b < TOTAL_CYCLES {
            let chunk = chunks[idx % chunks.len()].min(TOTAL_CYCLES - stepped_b);
            clock_b.advance(chunk);
            stepped_b += chunk;
            idx += 1;
        }

        assert_eq!(clock_a.total_cycles(), TOTAL_CYCLES);
        assert_eq!(clock_b.total_cycles(), TOTAL_CYCLES);
        assert_eq!(clock_a.wall_time_ns(), clock_b.wall_time_ns());
        // 50,000 cycles at 10 MHz = 5,000,000 ns = 5 ms
        assert_eq!(clock_a.wall_time_ns(), 5_000_000);
    }

    #[test]
    fn test_virtual_clock_serde_roundtrip() {
        let mut clock = VirtualClock::new(25_000_000).unwrap();
        clock.advance(123_456);

        let json = serde_json::to_string(&clock).unwrap();
        let restored: VirtualClock = serde_json::from_str(&json).unwrap();

        assert_eq!(clock, restored);
        assert_eq!(restored.frequency_hz(), 25_000_000);
        assert_eq!(restored.total_cycles(), 123_456);
    }
}
