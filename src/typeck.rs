/// type inference in HM style
use std::{collections::HashMap, iter, mem};

use crate::{PtrMap, Visitor};

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
enum Prim {
    Int,
    Bool,
    Unit,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
enum Type {
    Con(Prim),
    Fun(Vec<Type>, Box<Type>),
    Tuple(Vec<Type>),
    Var(MetaVar),
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct MetaVar(usize);

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct Scheme {
    vars: Vec<MetaVar>,
    ty: Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Level(u32);

#[allow(dead_code)]
fn do_typeck(_program: &crate::Program) {
    enum VarKind {
        Unbound(Level),
        Link(Type),
    }

    struct InferEngine<'a> {
        current_level: Level,
        types: PtrMap<'a, Type>,
        var_kinds: Vec<VarKind>,
        errors: Vec<String>,
        schemes: PtrMap<'a, Scheme>,
        current_fn: Option<&'a crate::FnDecl>,
    }

    impl<'a> InferEngine<'a> {
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

        fn unify(&mut self, a: Type, b: Type) -> Result<(), String> {
            let ta = self.find(a);
            let tb = self.find(b);

            match (ta, tb) {
                (Type::Con(a), Type::Con(b)) if a == b => Ok(()),
                (Type::Fun(params_a, ret_a), Type::Fun(params_b, ret_b)) => {
                    if params_a.len() != params_b.len() {
                        return Err(format!(
                            "function arity mismatch: {} vs {}",
                            params_a.len(),
                            params_b.len()
                        ));
                    }

                    for (param_a, param_b) in params_a.into_iter().zip(params_b.into_iter()) {
                        self.unify(param_a, param_b)?;
                    }
                    self.unify(*ret_a, *ret_b)
                }
                (Type::Tuple(elems_a), Type::Tuple(elems_b)) => {
                    if elems_a.len() != elems_b.len() {
                        return Err(format!(
                            "tuple arity mismatch: {} vs {}",
                            elems_a.len(),
                            elems_b.len()
                        ));
                    }

                    for (elem_a, elem_b) in elems_a.into_iter().zip(elems_b.into_iter()) {
                        self.unify(elem_a, elem_b)?;
                    }
                    Ok(())
                }
                (ta @ Type::Var(_), tb @ _) | (ta @ _, tb @ Type::Var(_)) => {
                    if self.contains(&ta, &tb) {
                        return Err(format!("occurs check failed: {:?} occurs in {:?}", ta, tb));
                    }

                    self.link(ta, tb);
                    Ok(())
                }
                (ta, tb) => Err(format!("cannot unify types: {:?} vs {:?}", ta, tb)),
            }
        }

        fn contains(&self, ty: &Type, other: &Type) -> bool {
            if let Type::Var(meta) = other {
                match ty {
                    Type::Var(other_meta) => meta == other_meta,
                    Type::Con(_) => false,
                    Type::Fun(params, ret) => {
                        params.iter().any(|param| self.contains(param, other))
                            || self.contains(ret, other)
                    }
                    Type::Tuple(elems) => elems.iter().any(|elem| self.contains(elem, other)),
                }
            } else {
                return false;
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

        fn resolve<T>(&mut self, expr: &T) -> Type {
            let ty = self
                .types
                .get(expr)
                .expect("type should be inferred")
                .clone();

            self.zonk(ty)
        }

        fn zonk(&mut self, ty: Type) -> Type {
            match ty {
                Type::Var(MetaVar(idx)) => match &self.var_kinds[idx] {
                    VarKind::Unbound(_) => ty,
                    VarKind::Link(inner) => self.zonk(inner.clone()),
                },
                Type::Con(_) => ty,
                Type::Fun(params, ret) => Type::Fun(
                    params.into_iter().map(|param| self.zonk(param)).collect(),
                    Box::new(self.zonk(*ret)),
                ),
                Type::Tuple(elems) => {
                    Type::Tuple(elems.into_iter().map(|elem| self.zonk(elem)).collect())
                }
            }
        }
    }

    impl<'p> Visitor<'p> for InferEngine<'p> {
        fn visit_program(&mut self, _program: &'p crate::Program) {}

        fn visit_fn(&mut self, _decl: &'p crate::FnDecl) {
            self.current_fn = Some(_decl);
        }

        fn visit_block(&mut self, _block: &'p crate::Block) {}

        fn visit_stmt(&mut self, _stmt: &'p crate::Stmt) {}

        fn visit_stmt_post(&mut self, _stmt: &'p crate::Stmt) {
            match _stmt {
                crate::Stmt::Let { name, ty, init } => {
                    let init_ty = self
                        .types
                        .get(init)
                        .expect("init type should be inferred")
                        .clone();

                    if let Some(ty) = ty.as_ref().map(|ty| match ty {
                        crate::Type::Int => Type::Con(Prim::Int),
                        crate::Type::Bool => Type::Con(Prim::Bool),
                    }) {
                        if let Err(err) = self.unify(init_ty.clone(), ty.clone()) {
                            self.errors.push(err);
                        }
                    }

                    let scheme = self.generalize(init_ty);

                    self.schemes.insert(name.as_ref(), scheme);
                }
                crate::Stmt::Assign { name, value } => {
                    let value_ty = self
                        .types
                        .get(value)
                        .expect("value type should be inferred")
                        .clone();

                    let scheme = self
                        .schemes
                        .get(name.as_ref())
                        .expect("variable should be in scope")
                        .clone();
                    let var_ty = self.instantiate(&scheme);

                    if let Err(err) = self.unify(value_ty.clone(), var_ty.clone()) {
                        self.errors.push(err);
                    }
                }
                crate::Stmt::Expr(_) => todo!(),
                crate::Stmt::Return(v) => {
                    let ret_ty = v
                        .as_ref()
                        .map(|v| {
                            self.types
                                .get(v)
                                .expect("return value type should be inferred")
                                .clone()
                        })
                        .unwrap_or(Type::Con(Prim::Unit));

                    if let Some(current_fn) = self.current_fn {
                        let expected_ret_ty = match &current_fn.ret {
                            crate::Type::Int => Type::Con(Prim::Int),
                            crate::Type::Bool => Type::Con(Prim::Bool),
                        };

                        if let Err(err) = self.unify(ret_ty.clone(), expected_ret_ty.clone()) {
                            self.errors.push(err);
                        }
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
                        .get(var.as_ref())
                        .expect("variable should be in scope")
                        .clone();
                    let ty = self.instantiate(&scheme);
                    self.types.insert(_expr, ty);
                }
                crate::Expr::Binary { op, lhs, rhs } => match op {
                    crate::BinOp::Add
                    | crate::BinOp::Sub
                    | crate::BinOp::Mul
                    | crate::BinOp::Div => {
                        let tl = self
                            .types
                            .get(lhs)
                            .expect("lhs type should be inferred")
                            .clone();
                        let tr = self
                            .types
                            .get(rhs)
                            .expect("rhs type should be inferred")
                            .clone();

                        if let Err(err) = self.unify(tl.clone(), Type::Con(Prim::Int)) {
                            self.errors.push(err);
                        }
                        if let Err(err) = self.unify(tr.clone(), Type::Con(Prim::Int)) {
                            self.errors.push(err);
                        }

                        self.types.insert(_expr, Type::Con(Prim::Int));
                    }
                    crate::BinOp::And | crate::BinOp::Or => {
                        let tl = self
                            .types
                            .get(lhs)
                            .expect("lhs type should be inferred")
                            .clone();
                        let tr = self
                            .types
                            .get(rhs)
                            .expect("rhs type should be inferred")
                            .clone();

                        if let Err(err) = self.unify(tl.clone(), Type::Con(Prim::Bool)) {
                            self.errors.push(err);
                        }
                        if let Err(err) = self.unify(tr.clone(), Type::Con(Prim::Bool)) {
                            self.errors.push(err);
                        }

                        self.types.insert(_expr, Type::Con(Prim::Bool));
                    }
                },
                crate::Expr::Block(_) => {
                    self.types.insert(_expr, Type::Con(Prim::Unit));
                }
                crate::Expr::If {
                    cond,
                    then_branch,
                    else_branch,
                } => {
                    let condt = self.types.get(cond).expect("cond type should be inferred");
                    if let Err(err) = self.unify(condt.clone(), Type::Con(Prim::Bool)) {
                        self.errors.push(err);
                    }

                    let thent = self
                        .types
                        .get(then_branch)
                        .expect("then branch type should be inferred")
                        .clone();

                    if let Some(else_branch) = else_branch {
                        let elset = self
                            .types
                            .get(else_branch)
                            .expect("else branch type should be inferred");

                        if let Err(err) = self.unify(thent.clone(), elset.clone()) {
                            self.errors.push(err);
                        }
                    }

                    self.types.insert(_expr, thent);
                }
                crate::Expr::Call { callee, args } => {
                    let tf = self
                        .types
                        .get(callee)
                        .expect("callee type should be inferred")
                        .clone();

                    let Type::Fun(params, ret) = tf else {
                        self.errors.push(format!(
                            "callee {:?} is not a function",
                            self.types.get(callee)
                        ));
                        return;
                    };

                    let targs = args
                        .iter()
                        .map(|arg| {
                            self.types
                                .get(arg)
                                .expect("argument type should be inferred")
                                .clone()
                        })
                        .collect::<Vec<_>>();

                    if params.len() != targs.len() {
                        self.errors.push(format!(
                            "function arity mismatch: expected {} arguments, got {}",
                            params.len(),
                            targs.len()
                        ));
                    }

                    for (param, arg) in params.into_iter().zip(targs.into_iter()) {
                        if let Err(err) = self.unify(param, arg) {
                            self.errors.push(err);
                        }
                    }

                    self.types.insert(_expr, *ret);
                }
            }
        }
    }
}
