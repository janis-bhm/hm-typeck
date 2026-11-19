use std::rc::Rc;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Type {
    Int,
    Bool,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    And,
    Or,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Expr {
    IntLit(i64),
    BoolLit(bool),
    Var(Rc<String>),
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Block(Block),
    If {
        cond: Box<Expr>,
        then_branch: Block,
        else_branch: Option<Box<Expr>>,
    },
    Call {
        callee: Rc<String>,
        args: Vec<Expr>,
    },
}

impl Expr {
    pub fn int(value: i64) -> Self {
        Self::IntLit(value)
    }

    pub fn bool(value: bool) -> Self {
        Self::BoolLit(value)
    }

    pub fn var(name: impl Into<String>) -> Self {
        Self::Var(Rc::new(name.into()))
    }

    pub fn binary(op: BinOp, lhs: impl Into<Self>, rhs: impl Into<Self>) -> Self {
        Self::Binary {
            op,
            lhs: Box::new(lhs.into()),
            rhs: Box::new(rhs.into()),
        }
    }

    pub fn call(callee: impl Into<String>, args: Vec<Self>) -> Self {
        Self::Call {
            callee: Rc::new(callee.into()),
            args,
        }
    }

    pub fn block(block: Block) -> Self {
        Self::Block(block)
    }

    pub fn if_(cond: impl Into<Self>, then_branch: Block, else_branch: Option<Self>) -> Self {
        Self::If {
            cond: Box::new(cond.into()),
            then_branch,
            else_branch: else_branch.map(Box::new),
        }
    }
}

impl From<i64> for Expr {
    fn from(value: i64) -> Self {
        Self::IntLit(value)
    }
}

impl From<bool> for Expr {
    fn from(value: bool) -> Self {
        Self::BoolLit(value)
    }
}

impl From<Block> for Expr {
    fn from(block: Block) -> Self {
        Self::Block(block)
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Stmt {
    Let {
        name: Rc<String>,
        ty: Option<Type>,
        init: Expr,
    },
    Assign {
        name: Rc<String>,
        value: Expr,
    },
    Expr(Expr),
    Return(Option<Expr>),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

impl Block {
    pub fn new() -> Self {
        Self { stmts: Vec::new() }
    }

    pub fn from_stmts(stmts: Vec<Stmt>) -> Self {
        Self { stmts }
    }
}

impl Default for Block {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FnDecl {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub ret: Type,
    pub body: Block,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Program {
    pub functions: Vec<FnDecl>,
}
