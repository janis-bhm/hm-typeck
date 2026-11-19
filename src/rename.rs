use std::collections::HashMap;
use std::fmt;

use crate::ast::{Block, Expr, FnDecl, Program, Stmt};
use crate::ptr_map::PtrMap;
use crate::walk::{Visitor, walk_program};

/// Marker trait for AST nodes that can be renamed.
pub trait Renameable: 'static {
    fn is_renameable(&self) -> bool {
        true
    }
}

impl Renameable for FnDecl {}
impl Renameable for Stmt {
    fn is_renameable(&self) -> bool {
        matches!(self, Stmt::Let { .. } | Stmt::Assign { .. })
    }
}
impl Renameable for Expr {
    fn is_renameable(&self) -> bool {
        matches!(self, Expr::Var(_) | Expr::Call { .. })
    }
}

/// A unique, alpha-renamed identifier: the original name plus a globally
/// increasing disambiguating index.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Ident {
    pub name: String,
    pub id: usize,
}

impl Ident {
    pub fn new(name: impl Into<String>, id: usize) -> Self {
        Self {
            name: name.into(),
            id,
        }
    }
}

impl fmt::Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.name, self.id)
    }
}

/// Assigns globally unique names to declarations and records resolved names
/// for variable references, calls, and assignment LHSs by AST-node address.
/// Function and lambda parameters are also recorded by their parameter-tuple
/// address and can be retrieved with [`param_ident`] and
/// [`lambda_param_ident`].
///
/// Let bindings enter scope after their initializer is visited. Parameters
/// scope over the function body, and each block (including a block expression)
/// introduces a scope. Lambda parameters scope over the lambda body.
/// Shadowing is resolved from the innermost binding out.
pub struct Renamer<'p> {
    renames: PtrMap<'p, Ident>,
    scopes: Vec<(String, Ident)>,
    next_id: usize,
    block_marks: Vec<usize>,
    function_param_counts: Vec<usize>,
    pending_let_idents: Vec<Ident>,
    functions: HashMap<String, Ident>,
}

impl<'p> Renamer<'p> {
    pub fn new() -> Self {
        Self {
            renames: PtrMap::new(),
            scopes: Vec::new(),
            next_id: 0,
            block_marks: Vec::new(),
            function_param_counts: Vec::new(),
            pending_let_idents: Vec::new(),
            functions: HashMap::new(),
        }
    }

    /// Returns the node-address-to-identifier mappings collected so far.
    pub fn renames(&self) -> &PtrMap<'p, Ident> {
        &self.renames
    }

    /// Consumes the renamer and returns its node-address-to-identifier map.
    pub fn into_renames(self) -> PtrMap<'p, Ident> {
        self.renames
    }

    fn fresh(&mut self, name: &str) -> Ident {
        let id = self.next_id;
        self.next_id += 1;
        Ident::new(name, id)
    }

    fn resolve(&self, name: &str) -> Option<Ident> {
        self.scopes
            .iter()
            .rev()
            .find_map(|(scoped_name, ident)| (scoped_name == name).then(|| ident.clone()))
            .or_else(|| self.functions.get(name).cloned())
    }
}

impl Default for Renamer<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'p> Visitor<'p> for Renamer<'p> {
    fn visit_program(&mut self, program: &'p Program) {
        // Register declarations up front so calls can resolve regardless of
        // whether their declaration appears earlier or later in the program.
        for decl in &program.functions {
            let ident = self.fresh(&decl.name);
            self.renames.insert(decl, ident.clone());
            self.functions.insert(decl.name.clone(), ident);
        }
    }

    fn visit_fn(&mut self, decl: &'p FnDecl) {
        // A standalone walk_fn has not passed through visit_program, so give
        // its declaration an identifier here if it was not predeclared.
        if !self.renames.contains_key(decl) {
            let ident = self.fresh(&decl.name);
            self.renames.insert(decl, ident.clone());
            self.functions.insert(decl.name.clone(), ident);
        }

        self.function_param_counts.push(decl.params.len());
        for (index, (name, _)) in decl.params.iter().enumerate() {
            let ident = self.fresh(name);
            self.renames.insert(&decl.params[index], ident.clone());
            self.scopes.push((name.clone(), ident));
        }
    }

    fn visit_fn_post(&mut self, _decl: &'p FnDecl) {
        let param_count = self
            .function_param_counts
            .pop()
            .expect("visit_fn_post must follow visit_fn");
        for _ in 0..param_count {
            self.scopes.pop();
        }
    }

    fn visit_block(&mut self, _block: &'p Block) {
        self.block_marks.push(self.scopes.len());
    }

    fn visit_block_post(&mut self, _block: &'p Block) {
        let mark = self
            .block_marks
            .pop()
            .expect("visit_block_post must follow visit_block");
        self.scopes.truncate(mark);
    }

    fn visit_stmt(&mut self, stmt: &'p Stmt) {
        match stmt {
            Stmt::Let { name, .. } => {
                // Reserve the binder's id in preorder, but do not expose it to
                // the initializer until visit_stmt_post adds it to scope.
                let ident = self.fresh(name);
                self.pending_let_idents.push(ident);
            }
            Stmt::Assign { name, .. } => {
                // Resolve the LHS before walking the value, against the scope at
                // this point in the statement traversal.
                let ident = self.resolve(name).unwrap_or_else(|| self.fresh(name));
                self.renames.insert(stmt, ident);
            }
            _ => {}
        }
    }

    fn visit_stmt_post(&mut self, stmt: &'p Stmt) {
        if let Stmt::Let { name, .. } = stmt {
            let name = name.as_str();
            let ident = self
                .pending_let_idents
                .pop()
                .expect("visit_stmt_post must follow visit_stmt for a let");
            self.renames.insert(stmt, ident.clone());
            self.scopes.push((name.to_owned(), ident));
        }
    }

    fn visit_expr(&mut self, expr: &'p Expr) {
        if let Expr::Lambda { params, .. } = expr {
            self.block_marks.push(self.scopes.len());
            for (index, (name, _)) in params.iter().enumerate() {
                let ident = self.fresh(name);
                self.renames.insert(&params[index], ident.clone());
                self.scopes.push((name.to_string(), ident));
            }
        }

        let name = match expr {
            Expr::Var(name) => Some(name.as_str()),
            Expr::Call { callee, .. } => Some(callee.as_str()),
            _ => None,
        };

        if let Some(name) = name {
            // Unresolved names receive a fresh identifier, preserving the
            // source name while ensuring each occurrence remains unique.
            let ident = self.resolve(name).unwrap_or_else(|| self.fresh(name));
            self.renames.insert(expr, ident);
        }
    }

    fn visit_expr_post(&mut self, expr: &'p Expr) {
        if matches!(expr, Expr::Lambda { .. }) {
            let mark = self
                .block_marks
                .pop()
                .expect("lambda visit_expr_post must follow visit_expr");
            self.scopes.truncate(mark);
        }
    }
}

/// Alpha-renames all function declarations, variable references, calls, and
/// assignment LHSs in `program`, returning mappings keyed by the address of
/// each corresponding AST node. Function and lambda parameter identifiers are
/// keyed by their parameter-tuple addresses and can be retrieved with
/// [`param_ident`] and [`lambda_param_ident`].
pub fn rename(program: &Program) -> PtrMap<'_, Ident> {
    let mut renamer = Renamer::new();
    walk_program(&mut renamer, program);
    renamer.into_renames()
}

/// Returns the alpha-renamed `Ident` for a function's parameter by index,
/// as recorded in a renames map produced by this module.
pub fn param_ident<'p, 'r>(
    renames: &'r PtrMap<'p, Ident>,
    decl: &'p FnDecl,
    index: usize,
) -> Option<&'r Ident> {
    decl.params.get(index)?;
    renames.get(&decl.params[index])
}

/// Returns the alpha-renamed `Ident` for a lambda parameter by index, as
/// recorded in a renames map produced by this module.
pub fn lambda_param_ident<'p, 'r>(
    renames: &'r PtrMap<'p, Ident>,
    expr: &'p Expr,
    index: usize,
) -> Option<&'r Ident> {
    let Expr::Lambda { params, .. } = expr else {
        return None;
    };
    params.get(index)?;
    renames.get(&params[index])
}

#[cfg(test)]
mod tests {
    use crate::ast::{Expr, Program, Stmt, Type};
    use crate::builder::{block, block_expr, call, fn_decl, int, lambda, var};
    use crate::{InferType, Prim};

    use super::{Ident, lambda_param_ident, param_ident, rename};

    fn function_body(program: &Program, index: usize) -> &crate::ast::Block {
        &program.functions[index].body
    }

    #[test]
    fn renames_shadowed_bindings_at_their_references() {
        let program = Program {
            functions: vec![
                fn_decl("f")
                    .body(
                        block()
                            .let_("x", Type::Int, int(1))
                            .let_("y", Type::Int, var("x"))
                            .let_("x", Type::Int, int(2))
                            .let_("z", Type::Int, var("x"))
                            .finish(),
                    )
                    .finish(),
            ],
        };
        let map = rename(&program);
        let stmts = &function_body(&program, 0).stmts;
        let Stmt::Let {
            init: first_ref, ..
        } = &stmts[1]
        else {
            panic!("expected y declaration");
        };
        let Stmt::Let {
            init: second_ref, ..
        } = &stmts[3]
        else {
            panic!("expected z declaration");
        };
        let first = map.get(first_ref).expect("first x reference was renamed");
        let second = map.get(second_ref).expect("second x reference was renamed");

        assert_eq!(first, &Ident::new("x", 1));
        assert_eq!(second, &Ident::new("x", 3));
        assert_ne!(first, second);
        assert_eq!(map.len(), 7); // function, four let statements, and two references
    }

    #[test]
    fn resolves_parameter_references() {
        let program = Program {
            functions: vec![
                fn_decl("f")
                    .param("x", Type::Int)
                    .body(block().expr(var("x")).finish())
                    .finish(),
            ],
        };
        let map = rename(&program);
        let Stmt::Expr(reference) = &function_body(&program, 0).stmts[0] else {
            panic!("expected expression statement");
        };

        assert_eq!(map.get(reference), Some(&Ident::new("x", 1)));
    }

    #[test]
    fn parameter_idents_are_recorded_and_retrievable() {
        let program = Program {
            functions: vec![
                fn_decl("f")
                    .param("x", Type::Int)
                    .param("y", Type::Bool)
                    .finish(),
            ],
        };
        let decl = &program.functions[0];
        let map = rename(&program);

        assert_eq!(param_ident(&map, decl, 0), Some(&Ident::new("x", 1)));
        assert_eq!(param_ident(&map, decl, 1), Some(&Ident::new("y", 2)));
        assert_eq!(param_ident(&map, decl, 2), None);
    }

    #[test]
    fn parameter_ident_matches_scope_resolution() {
        let program = Program {
            functions: vec![
                fn_decl("f")
                    .param("x", Type::Int)
                    .body(block().expr(var("x")).finish())
                    .finish(),
            ],
        };
        let decl = &program.functions[0];
        let map = rename(&program);
        let Stmt::Expr(reference) = &decl.body.stmts[0] else {
            panic!("expected expression statement");
        };

        assert_eq!(map.get(reference), param_ident(&map, decl, 0));
    }

    #[test]
    fn same_named_parameters_in_different_functions_get_distinct_idents() {
        let program = Program {
            functions: vec![
                fn_decl("f").param("x", Type::Int).finish(),
                fn_decl("g").param("x", Type::Int).finish(),
            ],
        };
        let map = rename(&program);
        let first = param_ident(&map, &program.functions[0], 0).unwrap();
        let second = param_ident(&map, &program.functions[1], 0).unwrap();

        assert_eq!(first, &Ident::new("x", 2));
        assert_eq!(second, &Ident::new("x", 3));
        assert_ne!(first, second);
    }

    #[test]
    fn call_resolves_to_function_declaration() {
        let program = Program {
            functions: vec![
                fn_decl("f").finish(),
                fn_decl("caller")
                    .body(block().expr(call("f", vec![])).finish())
                    .finish(),
            ],
        };
        let map = rename(&program);
        let declaration = &program.functions[0];
        let Stmt::Expr(call_expr) = &function_body(&program, 1).stmts[0] else {
            panic!("expected call expression statement");
        };

        assert_eq!(map.get(declaration), Some(&Ident::new("f", 0)));
        assert_eq!(map.get(call_expr), map.get(declaration));
    }

    #[test]
    fn lambda_can_monomorph() {
        let program = Program {
            functions: vec![
                fn_decl("caller")
                    .body(
                        block()
                            .let_infer(
                                "lambda",
                                lambda(vec![("x", None)], block().expr(var("x")).finish()),
                            )
                            .let_infer("result1", call("lambda", vec![int(1)]))
                            .let_infer("result2", call("lambda", vec![crate::builder::bool(false)]))
                            .expr(call("lambda", vec![int(3)]))
                            .finish(),
                    )
                    .finish(),
            ],
        };

        let info = crate::typeck::typeck(&program)
            .expect("let-bound lambda should support per-call instantiation");
        let stmts = &program.functions[0].body.stmts;
        let Stmt::Let { init: first, .. } = &stmts[1] else {
            panic!("expected result1 binding");
        };
        let Stmt::Let { init: second, .. } = &stmts[2] else {
            panic!("expected result2 binding");
        };
        assert_eq!(info.type_of(first), Some(InferType::Con(Prim::Int)));
        assert_eq!(info.type_of(second), Some(InferType::Con(Prim::Bool)));
    }

    #[test]
    fn lambda_parameters_shadow_outer_names_and_local_binding_shadows_function() {
        let program = Program {
            functions: vec![
                fn_decl("lambda").finish(),
                fn_decl("caller")
                    .body(
                        block()
                            .let_("x", Type::Int, int(0))
                            .let_infer(
                                "lambda",
                                lambda(
                                    vec![("x", Some(Type::Int)), ("y", None)],
                                    block().expr(var("x")).finish(),
                                ),
                            )
                            .expr(call("lambda", vec![int(1), int(2)]))
                            .finish(),
                    )
                    .finish(),
            ],
        };
        let map = rename(&program);
        let stmts = &program.functions[1].body.stmts;
        let Stmt::Let {
            init: lambda_expr @ Expr::Lambda { .. },
            ..
        } = &stmts[1]
        else {
            panic!("expected lambda initializer");
        };
        let Expr::Lambda { body, .. } = lambda_expr else {
            unreachable!();
        };
        let Stmt::Expr(param_ref) = &body.stmts[0] else {
            panic!("expected lambda body reference");
        };
        let Stmt::Expr(call_expr @ Expr::Call { .. }) = &stmts[2] else {
            panic!("expected call to local lambda");
        };

        let outer_x = map.get(&stmts[0]).expect("outer binding was renamed");
        let lambda_x = lambda_param_ident(&map, lambda_expr, 0).unwrap();
        let lambda_y = lambda_param_ident(&map, lambda_expr, 1).unwrap();
        let lambda_binding = map.get(&stmts[1]).expect("lambda binding was renamed");
        assert_eq!(map.get(param_ref), Some(lambda_x));
        assert_ne!(lambda_x, outer_x);
        assert_eq!(lambda_x.name, "x");
        assert_eq!(lambda_y.name, "y");
        assert_eq!(map.get(call_expr), Some(lambda_binding));
        assert_eq!(lambda_param_ident(&map, lambda_expr, 2), None);
        assert_eq!(lambda_param_ident(&map, param_ref, 0), None);
    }

    #[test]
    fn block_expression_has_a_nested_scope() {
        let program = Program {
            functions: vec![
                fn_decl("f")
                    .body(
                        block()
                            .let_("a", Type::Int, int(1))
                            .let_(
                                "b",
                                Type::Int,
                                block_expr(
                                    block().let_("a", Type::Int, int(2)).expr(var("a")).finish(),
                                ),
                            )
                            .finish(),
                    )
                    .finish(),
            ],
        };
        let map = rename(&program);
        let Stmt::Let {
            init: Expr::Block(inner_block),
            ..
        } = &function_body(&program, 0).stmts[1]
        else {
            panic!("expected block expression initializer");
        };
        let Stmt::Expr(reference) = &inner_block.stmts[1] else {
            panic!("expected inner variable reference");
        };

        assert_eq!(map.get(reference), Some(&Ident::new("a", 3)));
    }

    #[test]
    fn assignment_lhs_resolves_to_innermost_shadowed_binding() {
        let program = Program {
            functions: vec![
                fn_decl("f")
                    .body(
                        block()
                            .let_("x", Type::Int, int(1))
                            .let_(
                                "y",
                                Type::Int,
                                block_expr(
                                    block()
                                        .let_("x", Type::Int, int(2))
                                        .assign("x", int(3))
                                        .expr(var("x"))
                                        .finish(),
                                ),
                            )
                            .finish(),
                    )
                    .finish(),
            ],
        };
        let map = rename(&program);
        let Stmt::Let {
            init: Expr::Block(inner_block),
            ..
        } = &function_body(&program, 0).stmts[1]
        else {
            panic!("expected block expression initializer");
        };
        let assign_stmt = &inner_block.stmts[1];
        let Stmt::Expr(inner_reference) = &inner_block.stmts[2] else {
            panic!("expected inner variable reference");
        };

        let assignment_ident = map.get(assign_stmt).expect("assignment LHS was renamed");
        assert_eq!(assignment_ident.name, "x");
        assert_eq!(Some(assignment_ident), map.get(inner_reference));
        assert_eq!(assignment_ident, &Ident::new("x", 3));
    }

    #[test]
    fn let_initializer_resolves_before_its_binding_is_added() {
        let with_outer = Program {
            functions: vec![
                fn_decl("f")
                    .body(
                        block()
                            .let_("x", Type::Int, int(1))
                            .let_("x", Type::Int, var("x"))
                            .finish(),
                    )
                    .finish(),
            ],
        };
        let map = rename(&with_outer);
        let Stmt::Let { init, .. } = &function_body(&with_outer, 0).stmts[1] else {
            panic!("expected inner x declaration");
        };
        assert_eq!(map.get(init), Some(&Ident::new("x", 1)));

        let without_outer = Program {
            functions: vec![
                fn_decl("g")
                    .body(block().let_("x", Type::Int, var("x")).finish())
                    .finish(),
            ],
        };
        let map = rename(&without_outer);
        let let_stmt = &function_body(&without_outer, 0).stmts[0];
        let Stmt::Let { init, .. } = let_stmt else {
            panic!("expected x declaration");
        };
        assert_eq!(map.get(init), Some(&Ident::new("x", 2)));
        assert_ne!(map.get(init), map.get(let_stmt));
        assert_eq!(map.len(), 3); // function declaration, let statement, and unresolved reference
    }

    #[test]
    fn ident_display_includes_its_disambiguating_index() {
        assert_eq!(Ident::new("x", 3).to_string(), "x3");
    }
}
