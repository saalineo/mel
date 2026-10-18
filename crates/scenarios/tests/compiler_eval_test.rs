use mel_scenarios::{CompilerChallenge, Expr};

#[test]
fn test_compiler_arithmetic_precedence() {
    // 2 + 3 * 4 = 14
    let expr = Expr::Add(
        Box::new(Expr::Literal(2)),
        Box::new(Expr::Mul(
            Box::new(Expr::Literal(3)),
            Box::new(Expr::Literal(4)),
        )),
    );

    let expected = expr.eval();
    assert_eq!(expected, 14);

    let vm_res = CompilerChallenge::run_in_vm(&expr).expect("VM execution failed");
    assert_eq!(vm_res, expected);
}

#[test]
fn test_compiler_nested_parens() {
    // (1 + 2) * (3 + 4) = 21
    let expr = Expr::Mul(
        Box::new(Expr::Add(
            Box::new(Expr::Literal(1)),
            Box::new(Expr::Literal(2)),
        )),
        Box::new(Expr::Add(
            Box::new(Expr::Literal(3)),
            Box::new(Expr::Literal(4)),
        )),
    );

    let expected = expr.eval();
    assert_eq!(expected, 21);

    let vm_res = CompilerChallenge::run_in_vm(&expr).expect("VM execution failed");
    assert_eq!(vm_res, expected);
}

#[test]
fn test_compiler_division_and_subtraction() {
    // (100 - 25) / 5 = 15
    let expr = Expr::Div(
        Box::new(Expr::Sub(
            Box::new(Expr::Literal(100)),
            Box::new(Expr::Literal(25)),
        )),
        Box::new(Expr::Literal(5)),
    );

    let expected = expr.eval();
    assert_eq!(expected, 15);

    let vm_res = CompilerChallenge::run_in_vm(&expr).expect("VM execution failed");
    assert_eq!(vm_res, expected);
}

#[test]
fn test_compiler_if_else_branches() {
    // if 1 != 0 then 42 else 99 -> 42
    let expr_true = Expr::IfElse {
        cond: Box::new(Expr::Literal(1)),
        then_branch: Box::new(Expr::Literal(42)),
        else_branch: Box::new(Expr::Literal(99)),
    };
    assert_eq!(expr_true.eval(), 42);
    let res_true = CompilerChallenge::run_in_vm(&expr_true).expect("VM execution failed");
    assert_eq!(res_true, 42);

    // if 0 != 0 then 42 else 99 -> 99
    let expr_false = Expr::IfElse {
        cond: Box::new(Expr::Literal(0)),
        then_branch: Box::new(Expr::Literal(42)),
        else_branch: Box::new(Expr::Literal(99)),
    };
    assert_eq!(expr_false.eval(), 99);
    let res_false = CompilerChallenge::run_in_vm(&expr_false).expect("VM execution failed");
    assert_eq!(res_false, 99);
}
