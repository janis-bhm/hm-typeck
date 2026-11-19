use typeck::builder::{add, block, block_expr, bool, call, fn_decl, int, lambda, mul, var};
use typeck::{Expr, InferType, Prim, Program, Type, TypeError, typeck as infer};

fn program(functions: Vec<typeck::FnDecl>) -> Program {
    Program { functions }
}

fn has_error(errors: &[TypeError], predicate: impl Fn(&TypeError) -> bool) -> bool {
    errors.iter().any(predicate)
}

#[test]
fn infers_lets_operators_block_tails_and_function_signatures() {
    let first = fn_decl("first")
        .param("value", Type::Int)
        .body(block().ret(call("later", vec![var("value")])).finish())
        .finish();
    let later = fn_decl("later")
        .param("n", Type::Int)
        .body(
            block()
                .let_("twice", Type::Int, mul(var("n"), int(2)))
                .let_infer(
                    "answer",
                    block_expr(
                        block()
                            .let_infer("base", mul(var("twice"), int(2)))
                            .expr(add(var("base"), int(1)))
                            .finish(),
                    ),
                )
                .ret(var("answer"))
                .finish(),
        )
        .finish();
    let recursive = fn_decl("recursive")
        .param("n", Type::Int)
        .body(block().ret(call("recursive", vec![var("n")])).finish())
        .finish();
    let bool_fn = fn_decl("is_ready")
        .param("flag", Type::Bool)
        .ret_ty(Type::Bool)
        .body(
            block()
                .let_infer(
                    "same",
                    typeck::Expr::if_(
                        Expr::binary(typeck::BinOp::And, var("flag"), bool(true)),
                        block().expr(bool(true)).finish(),
                        Some(bool(false)),
                    ),
                )
                .ret(var("same"))
                .finish(),
        )
        .finish();
    let program = program(vec![first, later, recursive, bool_fn]);

    let info = infer(&program).expect("valid program");
    assert_eq!(
        info.signature("later"),
        Some(InferType::Fun(
            vec![InferType::Con(Prim::Int)],
            Box::new(InferType::Con(Prim::Int)),
        ))
    );
    let later_body = &program.functions[1].body;
    let infer_stmt = &later_body.stmts[1];
    let block_value = match infer_stmt {
        typeck::Stmt::Let { init, .. } => init,
        _ => unreachable!(),
    };
    let block_tail = match block_value {
        Expr::Block(block) => match &block.stmts[1] {
            typeck::Stmt::Expr(expr) => expr,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    assert_eq!(info.type_of(block_tail), Some(InferType::Con(Prim::Int)));
}

#[test]
fn infers_and_calls_a_local_lambda_binding() {
    let function = fn_decl("apply")
        .body(
            block()
                .let_infer(
                    "lambda",
                    lambda(
                        vec![("x", Some(Type::Int)), ("y", None)],
                        block().expr(add(var("x"), var("y"))).finish(),
                    ),
                )
                .let_infer("result", call("lambda", vec![int(2), int(3)]))
                .ret(var("result"))
                .finish(),
        )
        .finish();
    let program = program(vec![function]);

    let info = infer(&program).expect("lambda parameter should infer as Int");
    let typeck::Stmt::Let { init, .. } = &program.functions[0].body.stmts[0] else {
        unreachable!();
    };
    assert_eq!(
        info.type_of(init),
        Some(InferType::Fun(
            vec![InferType::Con(Prim::Int), InferType::Con(Prim::Int)],
            Box::new(InferType::Con(Prim::Int)),
        ))
    );
}

#[test]
fn reports_unbound_variables_annotations_conditions_and_branch_mismatches() {
    let unbound = program(vec![
        fn_decl("f")
            .body(block().ret(var("missing")).finish())
            .finish(),
    ]);
    assert!(has_error(
        &infer(&unbound).unwrap_err(),
        |error| matches!(error, TypeError::UnboundVariable(name) if name == "missing")
    ));

    let annotated = program(vec![
        fn_decl("f")
            .body(block().let_("x", Type::Int, bool(true)).finish())
            .finish(),
    ]);
    assert!(has_error(
        &infer(&annotated).unwrap_err(),
        |error| matches!(error, TypeError::TypeMismatch { .. })
    ));

    let bad_condition = program(vec![
        fn_decl("f")
            .body(
                block()
                    .expr(typeck::Expr::if_(int(1), block().finish(), None))
                    .finish(),
            )
            .finish(),
    ]);
    assert!(has_error(
        &infer(&bad_condition).unwrap_err(),
        |error| matches!(
            error,
            TypeError::TypeMismatch {
                expected: InferType::Con(Prim::Bool),
                ..
            }
        )
    ));

    let bad_branches = program(vec![
        fn_decl("f")
            .body(
                block()
                    .expr(typeck::Expr::if_(
                        bool(true),
                        block().expr(int(1)).finish(),
                        Some(bool(false)),
                    ))
                    .finish(),
            )
            .finish(),
    ]);
    assert!(has_error(
        &infer(&bad_branches).unwrap_err(),
        |error| matches!(error, TypeError::TypeMismatch { .. })
    ));
}

#[test]
fn reports_call_errors_and_return_mismatches() {
    let arity = program(vec![
        fn_decl("takes")
            .param("x", Type::Int)
            .body(block().ret(var("x")).finish())
            .finish(),
        fn_decl("caller")
            .body(block().expr(call("takes", vec![])).finish())
            .finish(),
    ]);
    assert!(has_error(&infer(&arity).unwrap_err(), |error| matches!(
        error,
        TypeError::ArityMismatch {
            expected: 1,
            found: 0
        }
    )));

    let not_fn = program(vec![
        fn_decl("f")
            .body(
                block()
                    .let_("value", Type::Int, int(2))
                    .expr(call("value", vec![]))
                    .finish(),
            )
            .finish(),
    ]);
    assert!(has_error(
        &infer(&not_fn).unwrap_err(),
        |error| matches!(error, TypeError::NotAFunction(name) if name == "value")
    ));

    let wrong_return = program(vec![
        fn_decl("f").body(block().ret(bool(true)).finish()).finish(),
    ]);
    assert!(has_error(
        &infer(&wrong_return).unwrap_err(),
        |error| matches!(
            error,
            TypeError::TypeMismatch {
                expected: InferType::Con(Prim::Int),
                found: InferType::Con(Prim::Bool)
            }
        )
    ));

    let bare_return = program(vec![
        fn_decl("f").body(block().ret_none().finish()).finish(),
    ]);
    assert!(has_error(
        &infer(&bare_return).unwrap_err(),
        |error| matches!(
            error,
            TypeError::TypeMismatch {
                expected: InferType::Con(Prim::Int),
                found: InferType::Con(Prim::Unit)
            }
        )
    ));
}

#[test]
fn lexical_scopes_shadow_and_do_not_leak_branch_bindings() {
    let shadowing = program(vec![
        fn_decl("f")
            .param("x", Type::Int)
            .body(
                block()
                    .let_("x", Type::Int, add(var("x"), int(1)))
                    .ret(var("x"))
                    .finish(),
            )
            .finish(),
    ]);
    assert!(infer(&shadowing).is_ok());

    let leaked = program(vec![
        fn_decl("f")
            .body(
                block()
                    .expr(typeck::Expr::if_(
                        bool(true),
                        block().let_("inner", Type::Int, int(1)).finish(),
                        None,
                    ))
                    .ret(var("inner"))
                    .finish(),
            )
            .finish(),
    ]);
    assert!(has_error(
        &infer(&leaked).unwrap_err(),
        |error| matches!(error, TypeError::UnboundVariable(name) if name == "inner")
    ));
}

#[test]
fn expression_type_lookup_resolves_substitutions() {
    let program = program(vec![
        fn_decl("f")
            .param("x", Type::Int)
            .body(
                block()
                    .let_infer("value", var("x"))
                    .ret(var("value"))
                    .finish(),
            )
            .finish(),
    ]);
    let info = infer(&program).expect("valid program");
    let var_expr = match &program.functions[0].body.stmts[1] {
        typeck::Stmt::Return(Some(expr)) => expr,
        _ => unreachable!(),
    };
    assert_eq!(info.type_of(var_expr), Some(InferType::Con(Prim::Int)));
}

#[test]
fn missing_else_is_unit_and_independent_errors_are_accumulated() {
    let no_else = program(vec![
        fn_decl("f")
            .body(
                block()
                    .let_infer(
                        "result",
                        Expr::if_(bool(true), block().expr(int(1)).finish(), None),
                    )
                    .finish(),
            )
            .finish(),
    ]);
    assert!(has_error(&infer(&no_else).unwrap_err(), |error| matches!(
        error,
        TypeError::TypeMismatch {
            expected: InferType::Con(Prim::Unit),
            found: InferType::Con(Prim::Int)
        }
    )));

    let invalid = program(vec![
        fn_decl("f")
            .body(
                block()
                    .let_("wrong", Type::Int, bool(true))
                    .expr(Expr::if_(int(1), block().finish(), None))
                    .finish(),
            )
            .finish(),
    ]);
    let errors = infer(&invalid).unwrap_err();
    assert!(
        errors
            .iter()
            .filter(|e| matches!(e, TypeError::TypeMismatch { .. }))
            .count()
            >= 2
    );
}

#[test]
fn assignments_to_non_value_initializers_share_one_monotype() {
    let program = program(vec![
        fn_decl("get").body(block().ret(int(1)).finish()).finish(),
        fn_decl("caller")
            .body(
                block()
                    .let_infer("x", call("get", vec![]))
                    .assign("x", bool(true))
                    .assign("x", int(1))
                    .ret(int(0))
                    .finish(),
            )
            .finish(),
    ]);

    let errors = infer(&program).expect_err("assigning Bool to an Int binding must fail");
    assert_eq!(
        errors
            .iter()
            .filter(|error| matches!(error, TypeError::TypeMismatch { .. }))
            .count(),
        1,
        "the later Int assignment agrees with the binding's pinned type"
    );
}
