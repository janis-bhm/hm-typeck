/// type inference in HM style
use std::{collections::HashMap, iter, mem};

use thiserror::Error;

use crate::{
    Ident, PtrMap, Visitor,
    rename::{self, Renameable},
    walk_program,
};

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prim {
    Int,
    Bool,
    Unit,
}

impl From<crate::ast::Type> for Prim {
    fn from(ty: crate::ast::Type) -> Self {
        match ty {
            crate::ast::Type::Int => Prim::Int,
            crate::ast::Type::Bool => Prim::Bool,
        }
    }
}

#[derive(Debug, Error)]
pub enum TypeError {
    #[error("unbound variable {0:?}")]
    UnboundVariable(String),
    #[error("type mismatch: expected {expected:?}, found {found:?}")]
    TypeMismatch { expected: Type, found: Type },
    #[error("cannot unify types: {0:?} vs {1:?}")]
    UnificationError(Type, Type),
    #[error("occurs check failed: {0:?} occurs in {1:?}")]
    OccursCheckFailed(MetaVar, Type),
    #[error("function arity mismatch: expected {expected} arguments, got {found}")]
    ArityMismatch { expected: usize, found: usize },
    #[error("callee {0:?} is not a function")]
    NotAFunction(String),
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Con(Prim),
    Fun(Vec<Type>, Box<Type>),
    Tuple(Vec<Type>),
    Var(MetaVar),
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MetaVar(usize);

/// A type produced by inference.
pub type InferType = Type;

/// Public results from type checking, indexed by AST node identity.
#[derive(Debug, Default)]
pub struct TypeckInfo<'a> {
    types: PtrMap<'a, Type>,
    signatures: HashMap<String, Type>,
    var_kinds: Vec<VarKind>,
}

impl<'a> TypeckInfo<'a> {
    /// Returns the inferred type for an expression in the checked program.
    pub fn type_of(&self, expr: &'a crate::Expr) -> Option<Type> {
        self.resolve(expr)
    }

    /// Returns a function's inferred signature by name.
    pub fn signature(&self, name: &str) -> Option<Type> {
        self.signatures.get(name).cloned()
    }

    fn resolve<T: 'static>(&self, expr: &T) -> Option<Type> {
        let ty = self.types.get(expr)?;

        Some(self.zonk(ty))
    }

    fn zonk(&self, ty: &Type) -> Type {
        match ty {
            Type::Var(MetaVar(idx)) => match &self.var_kinds[*idx] {
                VarKind::Unbound(_) => ty.clone(),
                VarKind::Link(inner) => self.zonk(inner),
            },
            Type::Con(_) => ty.clone(),
            Type::Fun(params, ret) => Type::Fun(
                params.iter().map(|param| self.zonk(param)).collect(),
                Box::new(self.zonk(ret)),
            ),
            Type::Tuple(elems) => Type::Tuple(elems.iter().map(|elem| self.zonk(elem)).collect()),
        }
    }
}

/// Type-checks a program and returns inferred expression types and signatures.
pub fn typeck<'a>(_program: &'a crate::Program) -> Result<TypeckInfo<'a>, Vec<TypeError>> {
    // TODO(user): connect the in-progress inference engine to this public API.
    let renames = crate::rename::rename(_program);

    let mut infer = InferEngine {
        renames: &renames,
        current_level: Level(0),
        types: PtrMap::new(),
        ident_types: HashMap::new(),
        var_kinds: Vec::new(),
        errors: Vec::new(),
        schemes: HashMap::new(),
        signatures: HashMap::new(),
        current_fn: None,
    };

    walk_program(&mut infer, _program);

    if infer.errors.is_empty() {
        Ok(TypeckInfo {
            types: infer.types,
            signatures: infer.signatures,
            var_kinds: infer.var_kinds,
        })
    } else {
        Err(infer.errors)
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct Scheme {
    vars: Vec<MetaVar>,
    ty: Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Level(u32);

#[derive(Debug, Clone)]
enum VarKind {
    Unbound(Level),
    Link(Type),
}

struct InferEngine<'a, 'rename> {
    renames: &'rename PtrMap<'a, Ident>,
    current_level: Level,
    types: PtrMap<'a, Type>,
    ident_types: HashMap<Ident, Type>,
    var_kinds: Vec<VarKind>,
    errors: Vec<TypeError>,
    schemes: HashMap<Ident, Scheme>,
    signatures: HashMap<String, Type>,
    current_fn: Option<&'a crate::FnDecl>,
}

impl<'a, 'rename> InferEngine<'a, 'rename> {
    fn find(&mut self, ty: Type) -> Type {
        match ty {
            Type::Var(MetaVar(idx)) => match &self.var_kinds[idx] {
                VarKind::Unbound(_) => ty,
                VarKind::Link(inner) => {
                    let resolved = self.find(inner.clone());

                    self.var_kinds[idx] = VarKind::Link(resolved.clone());

                    resolved
                }
            },
            _ => ty,
        }
    }

    fn fresh_var(&mut self) -> Type {
        let id = self.var_kinds.len();
        self.var_kinds.push(VarKind::Unbound(self.current_level));

        Type::Var(MetaVar(id))
    }

    fn set_level_in(&mut self, ty: &Type, level: Level) {
        fn set_level_in_recurse(ty: &Type, level: Level, var_kinds: &mut [VarKind]) {
            match ty {
                Type::Var(MetaVar(idx)) => match &var_kinds[*idx] {
                    VarKind::Unbound(Level(var_level)) if *var_level > level.0 => {
                        var_kinds[*idx] = VarKind::Unbound(level);
                    }
                    VarKind::Link(inner) => {
                        // SAFETY: this method recurses, `var_kinds[*idx]` is
                        // Link so a &mut ref to it will never be created,
                        // `var_kinds` is never pushed so it doesn't move, so
                        // this lifetime extension is safe.
                        let inner = unsafe { mem::transmute::<&Type, &Type>(inner) };
                        set_level_in_recurse(inner, level, var_kinds);
                    }
                    _ => {}
                },
                Type::Con(_) => {}
                Type::Fun(params, ret) => {
                    for param in params {
                        set_level_in_recurse(param, level, var_kinds);
                    }
                    set_level_in_recurse(ret, level, var_kinds);
                }
                Type::Tuple(elems) => {
                    for elem in elems {
                        set_level_in_recurse(elem, level, var_kinds);
                    }
                }
            }
        }

        set_level_in_recurse(ty, level, &mut self.var_kinds);
    }

    fn free_vars(&self, ty: &Type, level: Level) -> Vec<MetaVar> {
        fn recurse_ty(ty: &Type, level: Level, kinds: &[VarKind], acc: &mut Vec<MetaVar>) {
            match ty {
                Type::Var(MetaVar(idx)) => match &kinds[*idx] {
                    VarKind::Unbound(Level(bount_at)) => {
                        if *bount_at > level.0 {
                            acc.push(MetaVar(*idx));
                        }
                    }
                    VarKind::Link(inner) => recurse_ty(inner, level, kinds, acc),
                },
                Type::Con(_) => {}
                Type::Fun(params, ret) => {
                    for param in params {
                        recurse_ty(param, level, kinds, acc);
                    }
                    recurse_ty(ret, level, kinds, acc);
                }
                Type::Tuple(elems) => {
                    for elem in elems {
                        recurse_ty(elem, level, kinds, acc);
                    }
                }
            }
        }

        let mut acc = Vec::new();

        recurse_ty(ty, level, &self.var_kinds, &mut acc);

        acc
    }

    fn min_level_of(&self, ty: &Type) -> Level {
        fn min_level_of_recurse(ty: &Type, kinds: &[VarKind]) -> Level {
            match ty {
                Type::Var(MetaVar(idx)) => match &kinds[*idx] {
                    VarKind::Unbound(level) => *level,
                    VarKind::Link(inner) => min_level_of_recurse(inner, kinds),
                },
                Type::Con(_) => Level(0),
                Type::Fun(params, ret) => params
                    .iter()
                    .chain(iter::once(&**ret))
                    .map(|t| min_level_of_recurse(t, kinds))
                    .min()
                    .unwrap_or(Level(0)),
                Type::Tuple(elems) => elems
                    .iter()
                    .map(|elem| min_level_of_recurse(elem, kinds))
                    .min()
                    .unwrap_or(Level(0)),
            }
        }

        min_level_of_recurse(ty, &self.var_kinds)
    }

    fn link(&mut self, a: Type, b: Type) {
        let a = self.find(a);
        let b = self.find(b);

        match (a, b) {
            (Type::Var(MetaVar(idx)), ty) | (ty, Type::Var(MetaVar(idx))) => {
                if let VarKind::Unbound(level) = self.var_kinds[idx] {
                    let level = self.min_level_of(&ty).min(level);
                    self.set_level_in(&ty, level);
                    self.var_kinds[idx] = VarKind::Link(ty);
                    return;
                }
            }
            _ => {}
        }

        panic!("cannot link non-variable types or already linked variables");
    }

    fn generalize(&mut self, ty: Type) -> Scheme {
        let free_vars = self.free_vars(&ty, self.current_level);

        Scheme {
            vars: free_vars,
            ty,
        }
    }

    fn record_unify(&mut self, a: Type, b: Type) {
        if let Err(err) = self.unify(a, b) {
            self.errors.push(err);
        }
    }

    fn unify(&mut self, a: Type, b: Type) -> Result<(), TypeError> {
        let ta = self.find(a);
        let tb = self.find(b);

        match (ta, tb) {
            (Type::Con(a), Type::Con(b)) if a == b => Ok(()),
            (Type::Fun(params_a, ret_a), Type::Fun(params_b, ret_b)) => {
                if params_a.len() != params_b.len() {
                    return Err(TypeError::ArityMismatch {
                        expected: params_a.len(),
                        found: params_b.len(),
                    });
                }

                for (param_a, param_b) in params_a.into_iter().zip(params_b) {
                    self.unify(param_a, param_b)?;
                }
                self.unify(*ret_a, *ret_b)
            }
            (Type::Tuple(elems_a), Type::Tuple(elems_b)) => {
                if elems_a.len() != elems_b.len() {
                    return Err(TypeError::UnificationError(
                        Type::Tuple(elems_a),
                        Type::Tuple(elems_b),
                    ));
                }

                for (elem_a, elem_b) in elems_a.into_iter().zip(elems_b) {
                    self.unify(elem_a, elem_b)?;
                }
                Ok(())
            }
            (Type::Var(meta_a), Type::Var(meta_b)) if meta_a == meta_b => Ok(()),
            (Type::Var(meta), ta) | (ta, Type::Var(meta)) => {
                if self.occurs(meta, &ta) {
                    return Err(TypeError::OccursCheckFailed(meta, ta));
                }

                self.link(ta, Type::Var(meta));
                Ok(())
            }
            (ta, tb) => Err(TypeError::TypeMismatch {
                expected: tb,
                found: ta,
            }),
        }
    }

    fn node_type<T: 'static>(&self, node: &T) -> Type {
        self.types
            .get(node)
            .cloned()
            .unwrap_or(Type::Con(Prim::Unit))
    }

    fn ident_type<T: Renameable>(&self, node: &T) -> Type {
        let ident = self.node_ident(node);
        self.ident_types
            .get(ident)
            .cloned()
            .unwrap_or(Type::Con(Prim::Unit))
    }

    fn node_ident<T: Renameable>(&self, node: &T) -> &Ident {
        assert!(node.is_renameable(), "invalid node for alpha-renaming");
        self.renames.get(node).expect("node should be renamed")
    }

    fn occurs(&self, var: MetaVar, ty: &Type) -> bool {
        match ty {
            Type::Var(other) => {
                if *other == var {
                    true
                } else {
                    match &self.var_kinds[other.0] {
                        VarKind::Unbound(_) => false,
                        VarKind::Link(inner) => self.occurs(var, inner),
                    }
                }
            }
            Type::Con(_) => false,
            Type::Fun(params, ret) => {
                params.iter().any(|param| self.occurs(var, param)) || self.occurs(var, ret)
            }
            Type::Tuple(elems) => elems.iter().any(|elem| self.occurs(var, elem)),
        }
    }

    fn instantiate(&mut self, scheme: &Scheme) -> Type {
        let mut subst = HashMap::new();

        for var in &scheme.vars {
            subst.insert(*var, self.fresh_var());
        }

        fn instantiate_recurse(ty: &Type, subst: &HashMap<MetaVar, Type>) -> Type {
            match ty {
                Type::Var(var) => subst.get(var).cloned().unwrap_or(ty.clone()),
                Type::Con(_) => ty.clone(),
                Type::Fun(params, ret) => Type::Fun(
                    params
                        .iter()
                        .map(|param| instantiate_recurse(param, subst))
                        .collect(),
                    Box::new(instantiate_recurse(ret, subst)),
                ),
                Type::Tuple(elems) => Type::Tuple(
                    elems
                        .iter()
                        .map(|elem| instantiate_recurse(elem, subst))
                        .collect(),
                ),
            }
        }

        instantiate_recurse(&scheme.ty, &subst)
    }
}

impl<'p, 'rename> Visitor<'p> for InferEngine<'p, 'rename> {
    fn visit_program(&mut self, _program: &'p crate::Program) {
        self.signatures.extend(_program.functions.iter().map(|f| {
            let param_tys = f
                .params
                .iter()
                .map(|(_, ty)| Prim::from(*ty))
                .map(Type::Con)
                .collect::<Vec<_>>();
            let ret_ty = Type::Con(Prim::from(f.ret));

            let ty = Type::Fun(param_tys.clone(), Box::new(ret_ty.clone()));

            self.ident_types.insert(
                self.renames
                    .get(f)
                    .expect("function should be renamed")
                    .clone(),
                ty.clone(),
            );

            (f.name.clone(), ty)
        }));
    }

    fn visit_fn(&mut self, _decl: &'p crate::FnDecl) {
        let param_tys = _decl
            .params
            .iter()
            .map(|(_, ty)| Prim::from(*ty))
            .map(Type::Con)
            .collect::<Vec<_>>();

        for (i, param_ty) in param_tys.iter().enumerate() {
            let param_ident =
                rename::param_ident(self.renames, _decl, i).expect("parameter should be renamed");

            self.ident_types
                .insert(param_ident.clone(), param_ty.clone());

            self.schemes.insert(
                param_ident.clone(),
                Scheme {
                    vars: Vec::new(),
                    ty: param_ty.clone(),
                },
            );
        }

        let ret_ty = Type::Con(Prim::from(_decl.ret));

        let ty = Type::Fun(param_tys.clone(), Box::new(ret_ty.clone()));
        self.ident_types.insert(
            self.renames
                .get(_decl)
                .expect("function should be renamed")
                .clone(),
            ty.clone(),
        );

        self.current_level.0 += 1;
        self.current_fn = Some(_decl);
    }

    fn visit_fn_post(&mut self, _decl: &'p crate::FnDecl) {
        // nesting?
        self.current_level.0 -= 1;
        self.current_fn = None;

        let body_ty = _decl
            .body
            .trailing_return_position_expr()
            .map_or(Type::Con(Prim::Unit), |expr| self.node_type(expr));

        let ret_ty = Type::Con(Prim::from(_decl.ret));

        self.record_unify(body_ty.clone(), ret_ty.clone());
    }

    fn visit_expr(&mut self, expr: &'p crate::Expr) {
        if let crate::Expr::Lambda { params, .. } = expr {
            self.current_level.0 += 1;
            for (index, (_, annotation)) in params.iter().enumerate() {
                let param_ty = match annotation {
                    Some(ty) => Type::Con(Prim::from(*ty)),
                    None => self.fresh_var(),
                };
                let ident = rename::lambda_param_ident(self.renames, expr, index)
                    .expect("lambda parameter should be renamed")
                    .clone();
                self.ident_types.insert(ident.clone(), param_ty.clone());
                self.schemes.insert(
                    ident,
                    Scheme {
                        vars: Vec::new(),
                        ty: param_ty,
                    },
                );
            }
        }
    }

    fn visit_block(&mut self, _block: &'p crate::Block) {
        self.current_level.0 += 1;
    }

    fn visit_block_post(&mut self, _block: &'p crate::Block) {
        self.current_level.0 -= 1;

        let ty = _block
            .stmts
            .last()
            .map_or(Type::Con(Prim::Unit), |last_stmt| match last_stmt {
                crate::Stmt::Expr(expr) => self.node_type(expr),
                _ => Type::Con(Prim::Unit),
            });

        self.types.insert(_block, ty);
    }

    fn visit_stmt(&mut self, _stmt: &'p crate::Stmt) {
        if let crate::Stmt::Let { .. } = _stmt {
            self.current_level.0 += 1;
        }
    }

    fn visit_stmt_post(&mut self, _stmt: &'p crate::Stmt) {
        match _stmt {
            crate::Stmt::Let { ty, init, .. } => {
                let init_ty = self.node_type(init);
                self.current_level.0 -= 1;

                let ty = if let Some(ty) = ty.as_ref().map(|ty| match ty {
                    crate::Type::Int => Type::Con(Prim::Int),
                    crate::Type::Bool => Type::Con(Prim::Bool),
                }) {
                    self.record_unify(init_ty.clone(), ty.clone());
                    ty
                } else {
                    init_ty
                };

                let scheme = if is_value(init) {
                    self.generalize(ty.clone())
                } else {
                    Scheme {
                        vars: Vec::new(),
                        ty: ty.clone(),
                    }
                };

                let ident = self.node_ident(_stmt).clone();
                self.ident_types.insert(ident.clone(), ty.clone());
                self.schemes.insert(ident, scheme);

                self.types.insert(_stmt, Type::Con(Prim::Unit));
            }
            crate::Stmt::Assign { value, .. } => {
                let value_ty = self.node_type(value);

                if let Some(scheme) = self.schemes.get(self.node_ident(_stmt)).cloned() {
                    let var_ty = scheme.ty.clone();

                    if let Err(err) = self.unify(value_ty.clone(), var_ty.clone()) {
                        self.errors.push(err);
                    }
                }
            }
            crate::Stmt::Expr(_) => {
                // block checks the inner expr type, so we don't have to do anything here
            }
            crate::Stmt::Return(v) => {
                let ret_ty = v
                    .as_ref()
                    .map(|v| self.node_type(v))
                    .unwrap_or(Type::Con(Prim::Unit));

                if let Some(current_fn) = self.current_fn {
                    let expected_ret_ty = match &current_fn.ret {
                        crate::Type::Int => Type::Con(Prim::Int),
                        crate::Type::Bool => Type::Con(Prim::Bool),
                    };

                    self.record_unify(ret_ty.clone(), expected_ret_ty.clone());
                } else {
                    panic!("return statement outside of function");
                }
            }
        }

        self.types.insert(_stmt, Type::Con(Prim::Unit));
    }

    fn visit_expr_post(&mut self, _expr: &'p crate::Expr) {
        #[allow(unused_variables)]
        match _expr {
            crate::Expr::IntLit(_) => {
                self.types.insert(_expr, Type::Con(Prim::Int));
            }
            crate::Expr::BoolLit(_) => {
                self.types.insert(_expr, Type::Con(Prim::Bool));
            }
            crate::Expr::Var(var) => {
                let scheme = self
                    .schemes
                    .get(self.node_ident(_expr))
                    .cloned()
                    .unwrap_or_else(|| {
                        self.errors
                            .push(TypeError::UnboundVariable(var.to_string()));
                        Scheme {
                            vars: Vec::new(),
                            ty: Type::Con(Prim::Unit),
                        }
                    });
                let ty = self.instantiate(&scheme);
                self.types.insert(_expr, ty);
            }
            crate::Expr::Binary { op, lhs, rhs } => {
                let prim = match op {
                    crate::BinOp::Add
                    | crate::BinOp::Sub
                    | crate::BinOp::Mul
                    | crate::BinOp::Div => Prim::Int,
                    crate::BinOp::And | crate::BinOp::Or => Prim::Bool,
                };

                let tl = self.node_type(lhs.as_ref());
                let tr = self.node_type(rhs.as_ref());

                self.record_unify(tl.clone(), Type::Con(prim.clone()));
                self.record_unify(tr.clone(), Type::Con(prim.clone()));

                self.types.insert(_expr, Type::Con(prim));
            }
            crate::Expr::Block(block) => {
                let block_ty = self.node_type(block);
                self.types.insert(_expr, block_ty);
            }
            crate::Expr::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let condt = self.node_type(cond.as_ref());
                self.record_unify(condt.clone(), Type::Con(Prim::Bool));

                let thent = self.node_type(then_branch);
                let elset = else_branch
                    .as_ref()
                    .map(|else_branch| self.node_type(else_branch.as_ref()))
                    .unwrap_or(Type::Con(Prim::Unit));

                self.record_unify(thent.clone(), elset.clone());

                self.types.insert(_expr, thent);
            }
            crate::Expr::Lambda { params, body } => {
                let param_tys = (0..params.len())
                    .map(|index| {
                        let ident = rename::lambda_param_ident(self.renames, _expr, index)
                            .expect("lambda parameter should be renamed");
                        self.ident_types
                            .get(ident)
                            .cloned()
                            .expect("lambda parameter type should be inferred")
                    })
                    .collect();
                let body_ty = self.node_type(body);
                self.current_level.0 -= 1;
                self.types
                    .insert(_expr, Type::Fun(param_tys, Box::new(body_ty)));
            }
            crate::Expr::Call { callee, args } => {
                let tf = self
                    .schemes
                    .get(self.node_ident(_expr))
                    .cloned()
                    .map(|scheme| self.instantiate(&scheme))
                    .unwrap_or_else(|| self.ident_type(_expr));

                let Type::Fun(params, ret) = tf else {
                    self.errors
                        .push(TypeError::NotAFunction(callee.as_ref().clone()));
                    return;
                };

                let targs = args
                    .iter()
                    .map(|arg| self.node_type(arg))
                    .collect::<Vec<_>>();

                if params.len() != targs.len() {
                    self.errors.push(TypeError::ArityMismatch {
                        expected: params.len(),
                        found: targs.len(),
                    });
                }

                for (param, arg) in params.into_iter().zip(targs) {
                    self.record_unify(param, arg);
                }

                self.types.insert(_expr, *ret);
            }
        }
    }
}

/// Checks if an expression is a value for generalization purposes.
fn is_value(expr: &crate::Expr) -> bool {
    match expr {
        crate::Expr::IntLit(_) | crate::Expr::BoolLit(_) | crate::Expr::Lambda { .. } => true,
        crate::Expr::Block(block) => block.stmts.last().is_none_or(|stmt| match stmt {
            crate::Stmt::Expr(e) => is_value(e),
            _ => false,
        }),
        crate::Expr::If {
            then_branch,
            else_branch,
            ..
        } => {
            is_value(&crate::Expr::Block(then_branch.clone()))
                && else_branch
                    .as_ref()
                    .is_some_and(|else_expr| is_value(else_expr))
        }
        _ => false,
    }
}
