use crate::ast::{Block, Expr, FnDecl, Program, Stmt};

/// A visitor for AST nodes, called in preorder by the [`walk_*`] functions.
pub trait Visitor {
    fn visit_program(&mut self, _program: &Program) {}
    fn visit_fn(&mut self, _decl: &FnDecl) {}
    fn visit_block(&mut self, _block: &Block) {}
    fn visit_stmt(&mut self, _stmt: &Stmt) {}
    fn visit_expr(&mut self, _expr: &Expr) {}
}

pub fn walk_program<V: Visitor>(v: &mut V, program: &Program) {
    v.visit_program(program);
    for function in &program.functions {
        walk_fn(v, function);
    }
}

pub fn walk_fn<V: Visitor>(v: &mut V, decl: &FnDecl) {
    v.visit_fn(decl);
    walk_block(v, &decl.body);
}

pub fn walk_block<V: Visitor>(v: &mut V, block: &Block) {
    v.visit_block(block);
    for stmt in &block.stmts {
        walk_stmt(v, stmt);
    }
}

pub fn walk_stmt<V: Visitor>(v: &mut V, stmt: &Stmt) {
    v.visit_stmt(stmt);
    match stmt {
        Stmt::Let { init, .. } => walk_expr(v, init),
        Stmt::Assign { value, .. } => walk_expr(v, value),
        Stmt::Expr(expr) => walk_expr(v, expr),
        Stmt::Return(Some(expr)) => walk_expr(v, expr),
        Stmt::Return(None) => {}
    }
}

pub fn walk_expr<V: Visitor>(v: &mut V, expr: &Expr) {
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
}

#[cfg(test)]
mod tests {
    use super::{Visitor, walk_program};
    use crate::ast::{BinOp, Block, Expr, Program, Stmt, Type};
    use crate::builder::{add, block, block_expr, call, fn_decl, int, mul, var};

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
    }

    #[derive(Default)]
    struct Recorder(Vec<Event>);

    impl Visitor for Recorder {
        fn visit_program(&mut self, _program: &Program) {
            self.0.push(Event::Program);
        }

        fn visit_fn(&mut self, _decl: &crate::ast::FnDecl) {
            self.0.push(Event::Fn);
        }

        fn visit_block(&mut self, _block: &Block) {
            self.0.push(Event::Block);
        }

        fn visit_stmt(&mut self, stmt: &Stmt) {
            let event = match stmt {
                Stmt::Let { name, .. } => Event::Let(name.clone()),
                Stmt::Assign { name, .. } => Event::Assign(name.clone()),
                Stmt::Expr(_) => Event::ExprStmt,
                Stmt::Return(_) => Event::Return,
            };
            self.0.push(event);
        }

        fn visit_expr(&mut self, expr: &Expr) {
            let event = match expr {
                Expr::IntLit(value) => Event::ExprInt(*value),
                Expr::BoolLit(_) => Event::ExprBool,
                Expr::Var(name) => Event::ExprVar(name.clone()),
                Expr::Binary { op, .. } => Event::ExprBinary(op.clone()),
                Expr::Call { callee, .. } => Event::ExprCall(callee.clone()),
                Expr::Block(_) => Event::ExprBlock,
                Expr::If { .. } => Event::ExprIf,
            };
            self.0.push(event);
        }
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
