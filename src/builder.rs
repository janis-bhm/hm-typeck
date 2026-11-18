use crate::ast::{BinOp, Block, Expr, FnDecl, Program, Stmt, Type};

#[derive(Clone, Debug, Default)]
pub struct BlockBuilder {
    stmts: Vec<Stmt>,
}

impl BlockBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn let_(mut self, name: impl Into<String>, ty: Type, init: impl Into<Expr>) -> Self {
        self.stmts.push(Stmt::Let {
            name: name.into(),
            ty,
            init: init.into(),
        });
        self
    }

    pub fn assign(mut self, name: impl Into<String>, value: impl Into<Expr>) -> Self {
        self.stmts.push(Stmt::Assign {
            name: name.into(),
            value: value.into(),
        });
        self
    }

    pub fn expr(mut self, expr: impl Into<Expr>) -> Self {
        self.stmts.push(Stmt::Expr(expr.into()));
        self
    }

    pub fn if_(mut self, cond: impl Into<Expr>, then_branch: Block) -> Self {
        self.stmts.push(Stmt::Expr(if_expr(cond, then_branch)));
        self
    }

    pub fn if_else(
        mut self,
        cond: impl Into<Expr>,
        then_branch: Block,
        else_branch: Block,
    ) -> Self {
        self.stmts
            .push(Stmt::Expr(if_else_expr(cond, then_branch, else_branch)));
        self
    }

    pub fn ret(mut self, expr: impl Into<Expr>) -> Self {
        self.stmts.push(Stmt::Return(Some(expr.into())));
        self
    }

    pub fn ret_none(mut self) -> Self {
        self.stmts.push(Stmt::Return(None));
        self
    }

    pub fn finish(self) -> Block {
        Block::from_stmts(self.stmts)
    }
}

#[derive(Clone, Debug)]
pub struct FnBuilder {
    name: String,
    params: Vec<(String, Type)>,
    ret: Type,
    body: Block,
}

impl FnBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            params: Vec::new(),
            ret: Type::Int,
            body: Block::new(),
        }
    }

    pub fn param(mut self, name: impl Into<String>, ty: Type) -> Self {
        self.params.push((name.into(), ty));
        self
    }

    pub fn ret_ty(mut self, ty: Type) -> Self {
        self.ret = ty;
        self
    }

    pub fn body(mut self, body: Block) -> Self {
        self.body = body;
        self
    }

    pub fn finish(self) -> FnDecl {
        FnDecl {
            name: self.name,
            params: self.params,
            ret: self.ret,
            body: self.body,
        }
    }
}

pub fn block() -> BlockBuilder {
    BlockBuilder::new()
}

pub fn block_expr(block: Block) -> Expr {
    Expr::Block(block)
}

pub fn if_expr(cond: impl Into<Expr>, then_branch: Block) -> Expr {
    Expr::if_(cond, then_branch, None)
}

pub fn if_else_expr(cond: impl Into<Expr>, then_branch: Block, else_branch: Block) -> Expr {
    Expr::if_(cond, then_branch, Some(Expr::Block(else_branch)))
}

pub fn fn_decl(name: impl Into<String>) -> FnBuilder {
    FnBuilder::new(name)
}

pub fn int(value: i64) -> Expr {
    Expr::int(value)
}

pub fn bool(value: bool) -> Expr {
    Expr::bool(value)
}

pub fn var(name: impl Into<String>) -> Expr {
    Expr::var(name)
}

pub fn call(name: impl Into<String>, args: Vec<Expr>) -> Expr {
    Expr::call(name, args)
}

pub fn add(lhs: impl Into<Expr>, rhs: impl Into<Expr>) -> Expr {
    Expr::binary(BinOp::Add, lhs, rhs)
}

pub fn sub(lhs: impl Into<Expr>, rhs: impl Into<Expr>) -> Expr {
    Expr::binary(BinOp::Sub, lhs, rhs)
}

pub fn mul(lhs: impl Into<Expr>, rhs: impl Into<Expr>) -> Expr {
    Expr::binary(BinOp::Mul, lhs, rhs)
}

pub fn div(lhs: impl Into<Expr>, rhs: impl Into<Expr>) -> Expr {
    Expr::binary(BinOp::Div, lhs, rhs)
}

pub fn program(functions: Vec<FnDecl>) -> Program {
    Program { functions }
}
