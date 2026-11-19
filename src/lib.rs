pub mod ast;
pub mod builder;
pub mod print;
pub mod ptr_map;
pub mod rename;
pub mod typeck;
pub mod walk;

pub use ast::{BinOp, Block, Expr, FnDecl, Program, Stmt, Type};
pub use builder::{BlockBuilder, FnBuilder};
pub use print::{pretty, print_block, print_expr, print_stmt};
pub use ptr_map::{Ptr, PtrMap};
pub use rename::{Ident, Renamer, rename};
pub use typeck::{InferType, Prim, TypeError, TypeckInfo, typeck};
pub use walk::{Visitor, walk_block, walk_expr, walk_fn, walk_program, walk_stmt};

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::ast::{BinOp, Block, Expr, FnDecl, Program, Stmt, Type};
    use super::builder::{
        add, block, block_expr, bool, call, div, fn_decl, int, lambda, mul, sub, var,
    };
    use super::pretty;

    #[test]
    fn builders_construct_a_program() {
        let then_branch = block()
            .assign("result", mul(var("result"), int(3)))
            .finish();
        let else_branch = block()
            .assign("result", sub(var("result"), int(4)))
            .finish();
        let body = block()
            .let_("result", Type::Int, add(var("x"), int(2)))
            .let_("enabled", Type::Bool, bool(true))
            .if_else(var("enabled"), then_branch.clone(), else_branch.clone())
            .expr(call("record", vec![var("result")]))
            .ret(div(var("result"), int(2)))
            .finish();
        let actual = Program {
            functions: vec![
                fn_decl("f")
                    .param("x", Type::Int)
                    .param("flag", Type::Bool)
                    .ret_ty(Type::Int)
                    .body(body)
                    .finish(),
            ],
        };

        let expected = Program {
            functions: vec![FnDecl {
                name: "f".to_owned(),
                params: vec![("x".to_owned(), Type::Int), ("flag".to_owned(), Type::Bool)],
                ret: Type::Int,
                body: Block {
                    stmts: vec![
                        Stmt::Let {
                            name: Rc::new("result".to_owned()),
                            ty: Some(Type::Int),
                            init: Expr::Binary {
                                op: BinOp::Add,
                                lhs: Box::new(Expr::Var(Rc::new("x".to_owned()))),
                                rhs: Box::new(Expr::IntLit(2)),
                            },
                        },
                        Stmt::Let {
                            name: Rc::new("enabled".to_owned()),
                            ty: Some(Type::Bool),
                            init: Expr::BoolLit(true),
                        },
                        Stmt::Expr(Expr::If {
                            cond: Box::new(Expr::Var(Rc::new("enabled".to_owned()))),
                            then_branch,
                            else_branch: Some(Box::new(Expr::Block(else_branch))),
                        }),
                        Stmt::Expr(Expr::Call {
                            callee: Rc::new("record".to_owned()),
                            args: vec![Expr::Var(Rc::new("result".to_owned()))],
                        }),
                        Stmt::Return(Some(Expr::Binary {
                            op: BinOp::Div,
                            lhs: Box::new(Expr::Var(Rc::new("result".to_owned()))),
                            rhs: Box::new(Expr::IntLit(2)),
                        })),
                    ],
                },
            }],
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn lambda_builder_constructs_an_inferred_binding_and_call() {
        let init = lambda(
            vec![("x", Some(Type::Int)), ("y", None)],
            block().expr(add(var("x"), int(1))).finish(),
        );
        let body = block()
            .let_infer("lambda", init)
            .expr(call("lambda", vec![int(2)]))
            .finish();

        let Stmt::Let {
            name,
            ty: None,
            init:
                Expr::Lambda {
                    params,
                    body: lambda_body,
                },
        } = &body.stmts[0]
        else {
            panic!("expected inferred lambda binding");
        };
        assert_eq!(name.as_str(), "lambda");
        assert_eq!(
            params,
            &vec![
                (Rc::new("x".to_owned()), Some(Type::Int)),
                (Rc::new("y".to_owned()), None)
            ]
        );
        assert_eq!(lambda_body, &block().expr(add(var("x"), int(1))).finish());
        assert!(matches!(
            &body.stmts[1],
            Stmt::Expr(Expr::Call { callee, args })
                if callee.as_str() == "lambda" && args == &[Expr::IntLit(2)]
        ));
    }

    #[test]
    fn pretty_prints_rust_like_program() {
        let nested_if = Expr::if_(
            var("enabled"),
            block().expr(int(2)).finish(),
            Some(Expr::block(block().expr(int(3)).finish())),
        );
        let body = block()
            .let_("result", Type::Int, add(var("a"), int(2)))
            .assign("result", mul(var("result"), int(3)))
            .if_else(
                var("enabled"),
                block()
                    .assign("result", sub(var("result"), int(1)))
                    .finish(),
                block()
                    .assign("result", add(var("result"), int(4)))
                    .finish(),
            )
            .let_(
                "chosen",
                Type::Int,
                Expr::if_(
                    var("enabled"),
                    block().expr(int(1)).finish(),
                    Some(nested_if),
                ),
            )
            .let_(
                "from_block",
                Type::Int,
                block_expr(
                    block()
                        .let_("base", Type::Int, mul(var("a"), int(2)))
                        .expr(add(var("base"), int(1)))
                        .finish(),
                ),
            )
            .expr(call("record", vec![var("chosen"), var("from_block")]))
            .ret(add(var("result"), var("chosen")))
            .finish();
        let demo = fn_decl("demo")
            .param("a", Type::Int)
            .param("enabled", Type::Bool)
            .ret_ty(Type::Int)
            .body(body)
            .finish();
        let no_return_value = fn_decl("stop")
            .ret_ty(Type::Int)
            .body(block().ret_none().finish())
            .finish();
        let program = Program {
            functions: vec![demo, no_return_value],
        };

        assert_eq!(
            pretty(&program),
            r#"fn demo(a: Int, enabled: Bool) -> Int {
  let result: Int = a + 2;
  result = result * 3;
  if enabled {
    result = result - 1;
  } else {
    result = result + 4;
  };
  let chosen: Int = if enabled {
    1
  } else if enabled {
    2
  } else {
    3
  };
  let from_block: Int = {
    let base: Int = a * 2;
    base + 1
  };
  record(chosen, from_block);
  return result + chosen;
}

fn stop() -> Int {
  return;
}"#
        );
    }

    #[test]
    fn if_block_expression_can_initialize_a_let() {
        let expr = Expr::if_(
            bool(true),
            block().expr(int(1)).finish(),
            Some(block_expr(block().expr(int(2)).finish())),
        );
        let program = Program {
            functions: vec![
                fn_decl("value")
                    .body(block().let_("x", Type::Int, expr).ret(var("x")).finish())
                    .finish(),
            ],
        };

        assert_eq!(
            pretty(&program),
            "fn value() -> Int {\n  let x: Int = if true {\n    1\n  } else {\n    2\n  };\n  return x;\n}"
        );
    }
}
