use crate::ast::{BinOp, Block, Expr, FnDecl, Program, Stmt, Type};

fn indentation(level: usize) -> String {
    " ".repeat(level * 2)
}

fn type_name(ty: &Type) -> &'static str {
    match ty {
        Type::Int => "Int",
        Type::Bool => "Bool",
    }
}

fn operator(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
    }
}

fn precedence(op: &BinOp) -> u8 {
    match op {
        BinOp::Add | BinOp::Sub => 1,
        BinOp::Mul | BinOp::Div => 2,
    }
}

fn expression_precedence(expr: &Expr) -> u8 {
    match expr {
        Expr::Binary { op, .. } => precedence(op),
        _ => 3,
    }
}

fn print_expr_at(expr: &Expr, level: usize) -> String {
    match expr {
        Expr::IntLit(value) => value.to_string(),
        Expr::BoolLit(value) => value.to_string(),
        Expr::Var(name) => name.clone(),
        Expr::Call { callee, args } => {
            let args = args
                .iter()
                .map(|arg| print_expr_at(arg, level))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{callee}({args})")
        }
        Expr::Binary { op, lhs, rhs } => {
            let parent_precedence = precedence(op);
            let lhs_text = print_expr_at(lhs, level);
            let rhs_text = print_expr_at(rhs, level);
            let lhs_text = if expression_precedence(lhs) < parent_precedence {
                format!("({lhs_text})")
            } else {
                lhs_text
            };
            // Equal-precedence right operands need grouping to preserve the AST's association.
            let rhs_text = if expression_precedence(rhs) <= parent_precedence {
                format!("({rhs_text})")
            } else {
                rhs_text
            };
            format!("{lhs_text} {} {rhs_text}", operator(op))
        }
        Expr::Block(block) => print_braced_block(block, level, true),
        Expr::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let mut output = format!(
                "if {} {}",
                print_expr_at(cond, level),
                print_braced_block(then_branch, level, true)
            );
            if let Some(else_branch) = else_branch {
                match else_branch.as_ref() {
                    Expr::If { .. } => {
                        output.push_str(" else ");
                        output.push_str(&print_expr_at(else_branch, level));
                    }
                    Expr::Block(block) => {
                        output.push_str(" else ");
                        output.push_str(&print_braced_block(block, level, true));
                    }
                    expr => {
                        output.push_str(" else ");
                        output.push_str(&print_braced_expr(expr, level));
                    }
                }
            }
            output
        }
    }
}

fn print_braced_expr(expr: &Expr, level: usize) -> String {
    let inner = format!(
        "{}{}",
        indentation(level + 1),
        print_expr_at(expr, level + 1)
    );
    format!("{{\n{inner}\n{}}}", indentation(level))
}

fn print_braced_block(block: &Block, level: usize, tail_expression: bool) -> String {
    if block.stmts.is_empty() {
        return "{}".to_owned();
    }

    let mut lines = Vec::with_capacity(block.stmts.len());
    let last_index = block.stmts.len() - 1;
    for (index, stmt) in block.stmts.iter().enumerate() {
        let is_tail = tail_expression && index == last_index && matches!(stmt, Stmt::Expr(_));
        lines.push(print_stmt_at(stmt, level + 1, !is_tail));
    }

    format!("{{\n{}\n{}}}", lines.join("\n"), indentation(level))
}

fn print_stmt_at(stmt: &Stmt, level: usize, semicolon: bool) -> String {
    let prefix = indentation(level);
    match stmt {
        Stmt::Let { name, ty, init } => format!(
            "{prefix}let {name}: {} = {};",
            type_name(ty),
            print_expr_at(init, level)
        ),
        Stmt::Assign { name, value } => {
            format!("{prefix}{name} = {};", print_expr_at(value, level))
        }
        Stmt::Expr(expr) => {
            let suffix = if semicolon { ";" } else { "" };
            format!("{prefix}{}{suffix}", print_expr_at(expr, level))
        }
        Stmt::Return(Some(expr)) => {
            format!("{prefix}return {};", print_expr_at(expr, level))
        }
        Stmt::Return(None) => format!("{prefix}return;"),
    }
}

/// Print an expression without surrounding statement punctuation.
pub fn print_expr(expr: &Expr) -> String {
    print_expr_at(expr, 0)
}

/// Print a statement, including its terminating semicolon where appropriate.
pub fn print_stmt(stmt: &Stmt) -> String {
    print_stmt_at(stmt, 0, true)
}

/// Print a block with its braces and two-space indentation.
pub fn print_block(block: &Block) -> String {
    print_braced_block(block, 0, false)
}

fn print_function(function: &FnDecl) -> String {
    let params = function
        .params
        .iter()
        .map(|(name, ty)| format!("{name}: {}", type_name(ty)))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "fn {}({params}) -> {} {}",
        function.name,
        type_name(&function.ret),
        print_block(&function.body)
    )
}

/// Render a complete program as Rust-like source.
pub fn pretty(program: &Program) -> String {
    program
        .functions
        .iter()
        .map(print_function)
        .collect::<Vec<_>>()
        .join("\n\n")
}
