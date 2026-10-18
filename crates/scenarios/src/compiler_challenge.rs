//! Mini-Compiler Challenge Verification Framework.

use mel_core::{MelError, RegisterId};
use mel_vm::create_basic_riscv64_vm;
use serde::{Deserialize, Serialize};

/// High-level expression Abstract Syntax Tree (AST).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Expr {
    Literal(i64),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    IfElse {
        cond: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
    },
}

impl Expr {
    /// Directly evaluate expression in host environment to establish ground truth.
    pub fn eval(&self) -> i64 {
        match self {
            Self::Literal(val) => *val,
            Self::Add(lhs, rhs) => lhs.eval() + rhs.eval(),
            Self::Sub(lhs, rhs) => lhs.eval() - rhs.eval(),
            Self::Mul(lhs, rhs) => lhs.eval() * rhs.eval(),
            Self::Div(lhs, rhs) => {
                let r = rhs.eval();
                if r == 0 {
                    0
                } else {
                    lhs.eval() / r
                }
            }
            Self::IfElse {
                cond,
                then_branch,
                else_branch,
            } => {
                if cond.eval() != 0 {
                    then_branch.eval()
                } else {
                    else_branch.eval()
                }
            }
        }
    }
}

const REG_SP: u32 = 2;
const REG_T0: u32 = 5;
const REG_A0: u32 = 10;

/// Helper instruction encoders for RISC-V RV64
fn emit_addi(rd: u32, rs1: u32, imm: i32, code: &mut Vec<u8>) {
    // funct3 is 0b000 for ADDI
    let imm12 = (imm as u32) & 0xFFF;
    let inst = (imm12 << 20) | (rs1 << 15) | (rd << 7) | 0x13;
    code.extend_from_slice(&inst.to_le_bytes());
}

fn emit_sd(rs2: u32, rs1: u32, imm: i32, code: &mut Vec<u8>) {
    let imm = imm as u32;
    let imm11_5 = (imm >> 5) & 0x7F;
    let imm4_0 = imm & 0x1F;
    let inst = (imm11_5 << 25) | (rs2 << 20) | (rs1 << 15) | (0b011 << 12) | (imm4_0 << 7) | 0x23;
    code.extend_from_slice(&inst.to_le_bytes());
}

fn emit_ld(rd: u32, rs1: u32, imm: i32, code: &mut Vec<u8>) {
    let imm12 = (imm as u32) & 0xFFF;
    let inst = (imm12 << 20) | (rs1 << 15) | (0b011 << 12) | (rd << 7) | 0x03;
    code.extend_from_slice(&inst.to_le_bytes());
}

/// Reference RISC-V Code Generator and Challenge Evaluator.
pub struct CompilerChallenge;

impl CompilerChallenge {
    /// Emit RISC-V 32-bit instructions (4 bytes LE) pushing result to `a0`.
    pub fn compile_to_riscv(expr: &Expr) -> Vec<u8> {
        let mut code = Vec::new();

        // 1. Initialize sp = 0x80010000:
        // lui sp, 0x80010
        // slli sp, sp, 32
        // srli sp, sp, 32
        code.extend_from_slice(&[0x37, 0x01, 0x01, 0x80]);
        code.extend_from_slice(&[0x13, 0x11, 0x01, 0x02]);
        code.extend_from_slice(&[0x13, 0x51, 0x01, 0x02]);

        // 2. Compile AST into instructions leaving result in a0 (x10)
        Self::compile_node(expr, &mut code);

        // 3. Append spin loop (j .)
        code.extend_from_slice(&[0x6f, 0x00, 0x00, 0x00]);

        code
    }

    fn compile_node(expr: &Expr, code: &mut Vec<u8>) {
        match expr {
            Expr::Literal(val) => {
                emit_addi(REG_A0, 0, *val as i32, code); // addi a0, zero, val
            }
            Expr::Add(lhs, rhs) => {
                Self::compile_binary_op(lhs, rhs, 0b000, 0x33, 0, code); // add a0, t0, a0
            }
            Expr::Sub(lhs, rhs) => {
                Self::compile_binary_op(lhs, rhs, 0b000, 0x33, 0x20, code); // sub a0, t0, a0
            }
            Expr::Mul(lhs, rhs) => {
                Self::compile_binary_op(lhs, rhs, 0b000, 0x33, 0x01, code); // mul a0, t0, a0
            }
            Expr::Div(lhs, rhs) => {
                Self::compile_binary_op(lhs, rhs, 0b100, 0x33, 0x01, code); // div a0, t0, a0
            }
            Expr::IfElse {
                cond,
                then_branch,
                else_branch,
            } => {
                let branch = if cond.eval() != 0 {
                    then_branch
                } else {
                    else_branch
                };
                Self::compile_node(branch, code);
            }
        }
    }

    fn compile_binary_op(
        lhs: &Expr,
        rhs: &Expr,
        funct3: u32,
        opcode: u32,
        funct7: u32,
        code: &mut Vec<u8>,
    ) {
        // Compile lhs -> a0
        Self::compile_node(lhs, code);

        // Push a0 to stack: addi sp, sp, -16; sd a0, 0(sp)
        emit_addi(REG_SP, REG_SP, -16, code);
        emit_sd(REG_A0, REG_SP, 0, code);

        // Compile rhs -> a0
        Self::compile_node(rhs, code);

        // Pop lhs into t0 (x5): ld t0, 0(sp); addi sp, sp, 16
        emit_ld(REG_T0, REG_SP, 0, code);
        emit_addi(REG_SP, REG_SP, 16, code);

        // Op: rd=a0(10), rs1=t0(5), rs2=a0(10)
        let inst = (funct7 << 25)
            | (REG_A0 << 20)
            | (REG_T0 << 15)
            | (funct3 << 12)
            | (REG_A0 << 7)
            | opcode;
        code.extend_from_slice(&inst.to_le_bytes());
    }

    /// Execute expression compiled to RISC-V inside VM and return resulting a0.
    pub fn run_in_vm(expr: &Expr) -> Result<i64, MelError> {
        let code = Self::compile_to_riscv(expr);
        let mut vm = create_basic_riscv64_vm()?;
        vm.write_memory(0x8000_0000, &code)?;
        vm.set_pc(0x8000_0000)?;

        // Run for up to 500 cycles
        for _ in 0..500 {
            let res = vm.step()?;
            if let mel_cpu::traits::CpuStepResult::Halted { .. } = res {
                break;
            }
        }

        let a0 = vm.read_register(RegisterId::named("a0"))? as i64;
        Ok(a0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expression_ast_eval() {
        let expr = Expr::Sub(
            Box::new(Expr::Mul(
                Box::new(Expr::Add(
                    Box::new(Expr::Literal(10)),
                    Box::new(Expr::Literal(20)),
                )),
                Box::new(Expr::Literal(3)),
            )),
            Box::new(Expr::Literal(5)),
        );

        assert_eq!(expr.eval(), 85);
    }

    #[test]
    fn test_compiler_code_gen_and_execution() {
        // (2 + 3) * 4 = 20
        let expr = Expr::Mul(
            Box::new(Expr::Add(
                Box::new(Expr::Literal(2)),
                Box::new(Expr::Literal(3)),
            )),
            Box::new(Expr::Literal(4)),
        );

        let ground_truth = expr.eval();
        assert_eq!(ground_truth, 20);

        let vm_res = CompilerChallenge::run_in_vm(&expr).expect("VM run failed");
        assert_eq!(
            vm_res, ground_truth,
            "VM evaluation matches compiler code generator"
        );
    }
}
