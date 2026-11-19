use crate::ast::{Block, Expr, FnDecl, Program, Stmt};

/// A visitor for AST nodes called by the [`walk_*`] functions.
///
/// The `visit_*` methods run before a node's children are walked, and the
/// corresponding `*_post` methods run after its children have been visited.
///
/// The lifetime `'a` is the lifetime of the program borrow, and node references
/// passed to the visitor share it. This lets a visitor store node references in
/// an address-keyed `PtrMap<'a, V>`.
pub trait Visitor<'a> {
    fn visit_program(&mut self, _program: &'a Program) {}

    /// Called after the program's children have been visited.
    fn visit_program_post(&mut self, _program: &'a Program) {}

    fn visit_fn(&mut self, _decl: &'a FnDecl) {}

    /// Called after the function's children have been visited.
    fn visit_fn_post(&mut self, _decl: &'a FnDecl) {}

    fn visit_block(&mut self, _block: &'a Block) {}

    /// Called after the block's children have been visited.
    fn visit_block_post(&mut self, _block: &'a Block) {}

    fn visit_stmt(&mut self, _stmt: &'a Stmt) {}

    /// Called after the statement's children have been visited.
    fn visit_stmt_post(&mut self, _stmt: &'a Stmt) {}

    fn visit_expr(&mut self, _expr: &'a Expr) {}

    /// Called after the expression's children have been visited.
    fn visit_expr_post(&mut self, _expr: &'a Expr) {}
}

pub fn walk_program<'a, V: Visitor<'a>>(v: &mut V, program: &'a Program) {
    v.visit_program(program);
    for function in &program.functions {
        walk_fn(v, function);
    }
    v.visit_program_post(program);
}

pub fn walk_fn<'a, V: Visitor<'a>>(v: &mut V, decl: &'a FnDecl) {
    v.visit_fn(decl);
    walk_block(v, &decl.body);
    v.visit_fn_post(decl);
}

pub fn walk_block<'a, V: Visitor<'a>>(v: &mut V, block: &'a Block) {
    v.visit_block(block);
    for stmt in &block.stmts {
        walk_stmt(v, stmt);
    }
    v.visit_block_post(block);
}

pub fn walk_stmt<'a, V: Visitor<'a>>(v: &mut V, stmt: &'a Stmt) {
    v.visit_stmt(stmt);
    match stmt {
        Stmt::Let { init, .. } => walk_expr(v, init),
        Stmt::Assign { value, .. } => walk_expr(v, value),
        Stmt::Expr(expr) => walk_expr(v, expr),
        Stmt::Return(Some(expr)) => walk_expr(v, expr),
        Stmt::Return(None) => {}
    }
    v.visit_stmt_post(stmt);
}

pub fn walk_expr<'a, V: Visitor<'a>>(v: &mut V, expr: &'a Expr) {
    v.visit_expr(expr);
    match expr {
        Expr::IntLit(_) | Expr::BoolLit(_) | Expr::Var(_) => {}
        Expr::Binary { lhs, rhs, .. } => {
            walk_expr(v, lhs);
            walk_expr(v, rhs);
        }
        Expr::Call { args, .. } => {
            for arg in args {
                walk_expr(v, arg);
            }
        }
        Expr::Block(block) => walk_block(v, block),
        Expr::Lambda { body, .. } => walk_block(v, body),
        Expr::If {
            cond,
            then_branch,
            else_branch,
        } => {
            walk_expr(v, cond);
            walk_block(v, then_branch);
            if let Some(else_branch) = else_branch {
                walk_expr(v, else_branch);
            }
        }
    }
    v.visit_expr_post(expr);
}

#[cfg(test)]
mod tests {
    use super::{Visitor, walk_program};
    use crate::ast::{BinOp, Block, Expr, Program, Stmt, Type};
    use crate::builder::{add, block, block_expr, call, fn_decl, int, lambda, mul, var};

    #[derive(Debug, PartialEq, Eq)]
    enum Event {
        Program,
        Fn,
        Block,
        Let(String),
        Assign(String),
        ExprStmt,
        Return,
        ExprInt(i64),
        ExprBool,
        ExprVar(String),
        ExprBinary(BinOp),
        ExprCall(String),
        ExprBlock,
        ExprIf,
        ExprLambda,
        ExprLambdaPost,
    }

    #[derive(Default)]
    struct Recorder(Vec<Event>);

    impl<'p> Visitor<'p> for Recorder {
        fn visit_program(&mut self, _program: &'p Program) {
            self.0.push(Event::Program);
        }

        fn visit_fn(&mut self, _decl: &'p crate::ast::FnDecl) {
            self.0.push(Event::Fn);
        }

        fn visit_block(&mut self, _block: &'p Block) {
            self.0.push(Event::Block);
        }

        fn visit_stmt(&mut self, stmt: &'p Stmt) {
            let event = match stmt {
                Stmt::Let { name, .. } => Event::Let(name.clone().to_string()),
                Stmt::Assign { name, .. } => Event::Assign(name.clone().to_string()),
                Stmt::Expr(_) => Event::ExprStmt,
                Stmt::Return(_) => Event::Return,
            };
            self.0.push(event);
        }

        fn visit_expr(&mut self, expr: &'p Expr) {
            let event = match expr {
                Expr::IntLit(value) => Event::ExprInt(*value),
                Expr::BoolLit(_) => Event::ExprBool,
                Expr::Var(name) => Event::ExprVar(name.clone().to_string()),
                Expr::Binary { op, .. } => Event::ExprBinary(op.clone()),
                Expr::Call { callee, .. } => Event::ExprCall(callee.clone().to_string()),
                Expr::Block(_) => Event::ExprBlock,
                Expr::If { .. } => Event::ExprIf,
                Expr::Lambda { .. } => Event::ExprLambda,
            };
            self.0.push(event);
        }

        fn visit_expr_post(&mut self, expr: &'p Expr) {
            if matches!(expr, Expr::Lambda { .. }) {
                self.0.push(Event::ExprLambdaPost);
            }
        }
    }

    #[derive(Default)]
    struct ExprRefCollector<'p> {
        exprs: Vec<&'p Expr>,
    }

    impl<'p> Visitor<'p> for ExprRefCollector<'p> {
        fn visit_expr(&mut self, expr: &'p Expr) {
            self.exprs.push(expr);
        }
    }

    #[test]
    fn visitor_can_retain_expression_references_for_program_lifetime() {
        let program = Program {
            functions: vec![
                fn_decl("f")
                    .body(block().let_("x", Type::Int, add(int(1), int(2))).finish())
                    .finish(),
            ],
        };
        let mut collector = ExprRefCollector::default();

        walk_program(&mut collector, &program);

        assert_eq!(collector.exprs.len(), 3);
        assert!(matches!(collector.exprs[0], Expr::Binary { .. }));
        assert_eq!(collector.exprs[1], &Expr::IntLit(1));
        assert_eq!(collector.exprs[2], &Expr::IntLit(2));
    }

    #[test]
    fn visits_nested_arithmetic_in_preorder() {
        let program = Program {
            functions: vec![
                fn_decl("f")
                    .body(
                        block()
                            .let_("x", Type::Int, add(int(1), mul(int(2), int(3))))
                            .finish(),
                    )
                    .finish(),
            ],
        };
        let mut recorder = Recorder::default();

        walk_program(&mut recorder, &program);

        assert_eq!(
            recorder.0,
            vec![
                Event::Program,
                Event::Fn,
                Event::Block,
                Event::Let("x".to_owned()),
                Event::ExprBinary(BinOp::Add),
                Event::ExprInt(1),
                Event::ExprBinary(BinOp::Mul),
                Event::ExprInt(2),
                Event::ExprInt(3),
            ]
        );
    }

    #[test]
    fn visits_lambda_body_block_between_lambda_pre_and_post_events() {
        let program = Program {
            functions: vec![
                fn_decl("f")
                    .body(
                        block()
                            .let_infer(
                                "lambda",
                                lambda(
                                    vec![("x", None)],
                                    block().expr(add(var("x"), int(1))).finish(),
                                ),
                            )
                            .finish(),
                    )
                    .finish(),
            ],
        };
        let mut recorder = Recorder::default();

        walk_program(&mut recorder, &program);

        assert_eq!(
            recorder.0,
            vec![
                Event::Program,
                Event::Fn,
                Event::Block,
                Event::Let("lambda".to_owned()),
                Event::ExprLambda,
                Event::Block,
                Event::ExprStmt,
                Event::ExprBinary(BinOp::Add),
                Event::ExprVar("x".to_owned()),
                Event::ExprInt(1),
                Event::ExprLambdaPost,
            ]
        );
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Slot {
        Program,
        Fn,
        Block,
        Stmt,
        Add,
        Int(i64),
        Mul,
    }

    #[derive(Debug, PartialEq, Eq)]
    enum WalkEvent {
        Enter(Slot),
        Exit(Slot),
    }

    #[derive(Default)]
    struct PostorderRecorder(Vec<WalkEvent>);

    impl<'p> Visitor<'p> for PostorderRecorder {
        fn visit_program(&mut self, _program: &'p Program) {
            self.0.push(WalkEvent::Enter(Slot::Program));
        }

        fn visit_program_post(&mut self, _program: &'p Program) {
            self.0.push(WalkEvent::Exit(Slot::Program));
        }

        fn visit_fn(&mut self, _decl: &'p crate::ast::FnDecl) {
            self.0.push(WalkEvent::Enter(Slot::Fn));
        }

        fn visit_fn_post(&mut self, _decl: &'p crate::ast::FnDecl) {
            self.0.push(WalkEvent::Exit(Slot::Fn));
        }

        fn visit_block(&mut self, _block: &'p Block) {
            self.0.push(WalkEvent::Enter(Slot::Block));
        }

        fn visit_block_post(&mut self, _block: &'p Block) {
            self.0.push(WalkEvent::Exit(Slot::Block));
        }

        fn visit_stmt(&mut self, _stmt: &'p Stmt) {
            self.0.push(WalkEvent::Enter(Slot::Stmt));
        }

        fn visit_stmt_post(&mut self, _stmt: &'p Stmt) {
            self.0.push(WalkEvent::Exit(Slot::Stmt));
        }

        fn visit_expr(&mut self, expr: &'p Expr) {
            let slot = match expr {
                Expr::IntLit(value) => Slot::Int(*value),
                Expr::Binary { op: BinOp::Add, .. } => Slot::Add,
                Expr::Binary { op: BinOp::Mul, .. } => Slot::Mul,
                _ => unreachable!("test tree contains only integer and arithmetic expressions"),
            };
            self.0.push(WalkEvent::Enter(slot));
        }

        fn visit_expr_post(&mut self, expr: &'p Expr) {
            let slot = match expr {
                Expr::IntLit(value) => Slot::Int(*value),
                Expr::Binary { op: BinOp::Add, .. } => Slot::Add,
                Expr::Binary { op: BinOp::Mul, .. } => Slot::Mul,
                _ => unreachable!("test tree contains only integer and arithmetic expressions"),
            };
            self.0.push(WalkEvent::Exit(slot));
        }
    }

    #[test]
    fn visits_nodes_in_preorder_and_postorder() {
        let program = Program {
            functions: vec![
                fn_decl("f")
                    .body(
                        block()
                            .let_("x", Type::Int, add(int(1), mul(int(2), int(3))))
                            .finish(),
                    )
                    .finish(),
            ],
        };
        let mut recorder = PostorderRecorder::default();

        walk_program(&mut recorder, &program);

        assert_eq!(
            recorder.0,
            vec![
                WalkEvent::Enter(Slot::Program),
                WalkEvent::Enter(Slot::Fn),
                WalkEvent::Enter(Slot::Block),
                WalkEvent::Enter(Slot::Stmt),
                WalkEvent::Enter(Slot::Add),
                WalkEvent::Enter(Slot::Int(1)),
                WalkEvent::Exit(Slot::Int(1)),
                WalkEvent::Enter(Slot::Mul),
                WalkEvent::Enter(Slot::Int(2)),
                WalkEvent::Exit(Slot::Int(2)),
                WalkEvent::Enter(Slot::Int(3)),
                WalkEvent::Exit(Slot::Int(3)),
                WalkEvent::Exit(Slot::Mul),
                WalkEvent::Exit(Slot::Add),
                WalkEvent::Exit(Slot::Stmt),
                WalkEvent::Exit(Slot::Block),
                WalkEvent::Exit(Slot::Fn),
                WalkEvent::Exit(Slot::Program),
            ]
        );
    }

    #[test]
    fn visits_if_branches_in_source_order_with_nested_nodes() {
        let nested_if = Expr::if_(
            var("inner"),
            block().expr(int(2)).finish(),
            Some(block_expr(block().expr(int(3)).finish())),
        );
        let program = Program {
            functions: vec![
                fn_decl("f")
                    .body(
                        block()
                            .let_("value", Type::Int, call("add", vec![int(1), int(2)]))
                            .let_(
                                "from_block",
                                Type::Int,
                                block_expr(block().expr(int(4)).finish()),
                            )
                            .assign("value", int(5))
                            .expr(Expr::if_(
                                var("outer"),
                                block().expr(int(1)).finish(),
                                Some(nested_if),
                            ))
                            .ret(add(var("value"), int(6)))
                            .finish(),
                    )
                    .finish(),
            ],
        };
        let mut recorder = Recorder::default();

        walk_program(&mut recorder, &program);

        assert_eq!(
            recorder.0,
            vec![
                Event::Program,
                Event::Fn,
                Event::Block,
                Event::Let("value".to_owned()),
                Event::ExprCall("add".to_owned()),
                Event::ExprInt(1),
                Event::ExprInt(2),
                Event::Let("from_block".to_owned()),
                Event::ExprBlock, // block expression
                Event::Block,
                Event::ExprStmt,
                Event::ExprInt(4),
                Event::Assign("value".to_owned()),
                Event::ExprInt(5),
                Event::ExprStmt,
                Event::ExprIf,                      // outer if
                Event::ExprVar("outer".to_owned()), // outer condition
                Event::Block,                       // outer then block
                Event::ExprStmt,
                Event::ExprInt(1),
                Event::ExprIf, // nested if in the outer else branch
                Event::ExprVar("inner".to_owned()), // nested condition
                Event::Block,  // nested then block
                Event::ExprStmt,
                Event::ExprInt(2),
                Event::ExprBlock, // nested else block expression
                Event::Block,
                Event::ExprStmt,
                Event::ExprInt(3),
                Event::Return,
                Event::ExprBinary(BinOp::Add),      // return expression
                Event::ExprVar("value".to_owned()), // left operand
                Event::ExprInt(6),                  // right operand
            ]
        );
    }
}
