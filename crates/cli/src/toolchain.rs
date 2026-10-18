//! External Host Toolchain Auto-Detector and Environment Audit.

use mel_core::Architecture;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

/// Information about a detected host tool (compiler, assembler, linker, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub path: PathBuf,
    pub version: String,
    pub arch: Option<Architecture>,
}

/// Comprehensive report of detected tools and cross-compiler readiness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolchainReport {
    pub tools_found: Vec<ToolInfo>,
    pub missing_archs: Vec<Architecture>,
}

pub struct ToolchainDetector;

impl ToolchainDetector {
    /// List of standard tool candidate names mapped to target architecture.
    pub const CANDIDATES: &'static [(&'static str, Option<Architecture>)] = &[
        // RISC-V 64
        ("riscv64-unknown-elf-gcc", Some(Architecture::Riscv64)),
        ("riscv64-linux-gnu-gcc", Some(Architecture::Riscv64)),
        ("riscv64-unknown-elf-as", Some(Architecture::Riscv64)),
        ("riscv64-linux-gnu-as", Some(Architecture::Riscv64)),
        ("riscv64-unknown-elf-ld", Some(Architecture::Riscv64)),
        ("riscv64-unknown-elf-objcopy", Some(Architecture::Riscv64)),
        // ARM64
        ("aarch64-linux-gnu-gcc", Some(Architecture::Arm64)),
        ("aarch64-none-elf-gcc", Some(Architecture::Arm64)),
        ("aarch64-linux-gnu-as", Some(Architecture::Arm64)),
        ("aarch64-linux-gnu-ld", Some(Architecture::Arm64)),
        ("aarch64-linux-gnu-objcopy", Some(Architecture::Arm64)),
        // x86-64
        ("x86_64-elf-gcc", Some(Architecture::X86_64)),
        ("x86_64-linux-gnu-gcc", Some(Architecture::X86_64)),
        ("gcc", Some(Architecture::X86_64)),
        ("as", Some(Architecture::X86_64)),
        ("ld", Some(Architecture::X86_64)),
        ("objcopy", Some(Architecture::X86_64)),
        // Power ISA
        ("powerpc64le-linux-gnu-gcc", Some(Architecture::PowerIso64)),
        ("powerpc-linux-gnu-gcc", Some(Architecture::PowerIso64)),
        ("powerpc64le-linux-gnu-as", Some(Architecture::PowerIso64)),
        ("powerpc64le-linux-gnu-ld", Some(Architecture::PowerIso64)),
        // Generic LLVM
        ("clang", None),
        ("lld", None),
        ("llvm-objcopy", None),
    ];

    /// Parse first line of `--version` output to extract version string.
    pub fn parse_version_string(output: &str) -> String {
        output
            .lines()
            .next()
            .map(|l| l.trim().to_string())
            .unwrap_or_else(|| "Unknown version".to_string())
    }

    /// Check if a binary command exists in system PATH using `which` or standard lookup.
    pub fn find_tool(name: &str) -> Option<(PathBuf, String)> {
        let which_out = Command::new("which").arg(name).output().ok()?;
        if !which_out.status.success() {
            return None;
        }

        let path_str = String::from_utf8_lossy(&which_out.stdout)
            .trim()
            .to_string();
        if path_str.is_empty() {
            return None;
        }
        let path = PathBuf::from(path_str);

        let ver_out = Command::new(&path).arg("--version").output().ok();
        let version = match ver_out {
            Some(out) if out.status.success() => {
                Self::parse_version_string(&String::from_utf8_lossy(&out.stdout))
            }
            _ => "Installed (version unavailable)".to_string(),
        };

        Some((path, version))
    }

    /// Detect all available toolchains and return structured report.
    pub fn detect_all() -> ToolchainReport {
        let mut tools_found = Vec::new();
        let mut covered_archs = std::collections::HashSet::new();

        for (candidate, arch) in Self::CANDIDATES {
            if let Some((path, version)) = Self::find_tool(candidate) {
                if let Some(a) = arch {
                    covered_archs.insert(*a);
                }
                tools_found.push(ToolInfo {
                    name: (*candidate).to_string(),
                    path,
                    version,
                    arch: *arch,
                });
            }
        }

        let all_archs = [
            Architecture::Riscv64,
            Architecture::Arm64,
            Architecture::X86_64,
            Architecture::PowerIso64,
        ];

        let missing_archs = all_archs
            .iter()
            .copied()
            .filter(|a| !covered_archs.contains(a))
            .collect();

        ToolchainReport {
            tools_found,
            missing_archs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_string() {
        let sample = "gcc (Ubuntu 11.4.0-1ubuntu1~22.04) 11.4.0\nCopyright (C) 2021 Free Software Foundation, Inc.\n";
        assert_eq!(
            ToolchainDetector::parse_version_string(sample),
            "gcc (Ubuntu 11.4.0-1ubuntu1~22.04) 11.4.0"
        );

        let empty = "";
        assert_eq!(
            ToolchainDetector::parse_version_string(empty),
            "Unknown version"
        );
    }

    #[test]
    fn test_detect_common_tools() {
        let report = ToolchainDetector::detect_all();
        // Almost all dev linux systems have gcc, as, or ld
        println!("Detected tools: {}", report.tools_found.len());
        for t in &report.tools_found {
            println!("  [+] {} at {:?} ({})", t.name, t.path, t.version);
        }
        for m in &report.missing_archs {
            println!("  [-] Missing cross-toolchain for {:?}", m);
        }
    }
}
