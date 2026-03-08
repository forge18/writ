use super::*;

impl TypeChecker {
    pub fn check_stmt(&mut self, stmt: &Stmt) -> Result<(), TypeError> {
        match &stmt.kind {
            StmtKind::Let {
                name,
                type_annotation,
                initializer,
                ..
            } => self.check_variable_decl(
                name,
                type_annotation.as_ref(),
                initializer,
                &stmt.span,
                Mutability::Immutable,
            ),

            StmtKind::Var {
                name,
                type_annotation,
                initializer,
                ..
            } => self.check_variable_decl(
                name,
                type_annotation.as_ref(),
                initializer,
                &stmt.span,
                Mutability::Mutable,
            ),

            StmtKind::Const { name, initializer } => {
                let inferred = self.infer_expr(initializer)?;
                self.env.define(
                    name,
                    VarInfo {
                        ty: inferred,
                        mutability: Mutability::Constant,
                    },
                );
                Ok(())
            }

            StmtKind::Assignment { target, op, value } => self.check_assignment(target, op, value),

            StmtKind::ExprStmt(expr) => {
                self.infer_expr(expr)?;
                Ok(())
            }

            StmtKind::Block(stmts) => {
                self.env.push_scope();
                for s in stmts {
                    self.check_stmt(s)?;
                }
                self.env.pop_scope();
                Ok(())
            }

            StmtKind::If {
                condition,
                then_block,
                else_branch,
            } => self.check_if(condition, then_block, else_branch.as_ref()),

            StmtKind::While { condition, body } => self.check_while(condition, body),

            StmtKind::Return(value) => self.check_return(value.as_ref(), &stmt.span),

            StmtKind::Break | StmtKind::Continue => Ok(()),

            StmtKind::For { .. } => Err(TypeError::simple(
                "for-in loops are not supported yet".to_string(),
                stmt.span.clone(),
            )),

            StmtKind::When { subject, arms } => self.check_when(subject.as_ref(), arms, &stmt.span),

            StmtKind::LetDestructure { names, initializer } => {
                self.check_let_destructure(names, initializer, &stmt.span)
            }

            StmtKind::Func(func) => self.check_func_decl(func, &stmt.span),

            StmtKind::Class(decl) => self.check_class_decl(decl, &stmt.span),
            StmtKind::Trait(decl) => self.check_trait_decl(decl, &stmt.span),
            StmtKind::Enum(decl) => self.check_enum_decl(decl, &stmt.span),
            StmtKind::Struct(decl) => self.check_struct_decl(decl, &stmt.span),
            StmtKind::Import(import) => self.check_import(import, &stmt.span),
            StmtKind::WildcardImport(import) => self.check_wildcard_import(import, &stmt.span),
            StmtKind::Export(inner) => self.check_stmt(inner),

            StmtKind::Start(expr) => {
                self.infer_expr(expr)?;
                Ok(())
            }
        }
    }

    pub(super) fn check_variable_decl(
        &mut self,
        name: &str,
        type_annotation: Option<&TypeExpr>,
        initializer: &Expr,
        span: &Span,
        mutability: Mutability,
    ) -> Result<(), TypeError> {
        let inferred = self.infer_expr(initializer)?;

        let resolved = if let Some(annotation) = type_annotation {
            let annotated = self.resolve_type_expr(annotation, span)?;
            if !self.types_compatible(&annotated, &inferred) {
                return Err(TypeError::with_suggestions(
                    format!("type mismatch: expected {annotated}, found {inferred}"),
                    initializer.span.clone(),
                    suggestions::suggest_type_mismatch(&inferred, &annotated, &initializer.span),
                ));
            }
            annotated
        } else {
            inferred
        };

        self.env.define(
            name,
            VarInfo {
                ty: resolved,
                mutability,
            },
        );
        Ok(())
    }

    pub(super) fn check_assignment(
        &mut self,
        target: &Expr,
        op: &AssignOp,
        value: &Expr,
    ) -> Result<(), TypeError> {
        // Index assignment: collection[index] = value
        if let ExprKind::Index { object, index } = &target.kind {
            self.infer_expr(object)?;
            self.infer_expr(index)?;
            self.infer_expr(value)?;
            return Ok(());
        }

        // Member assignment: obj.field = value
        if let ExprKind::MemberAccess { .. } = &target.kind {
            self.infer_expr(target)?;
            self.infer_expr(value)?;
            return Ok(());
        }

        let name = match &target.kind {
            ExprKind::Identifier(name) => name,
            _ => {
                return Err(TypeError::simple(
                    "assignment target must be a variable".to_string(),
                    target.span.clone(),
                ));
            }
        };

        let var_info = self.env.lookup(name).ok_or_else(|| {
            let sugg = suggestions::suggest_variable_weighted(name, &self.env, None, &target.span);
            TypeError::with_suggestions(
                format!("undefined variable '{name}'"),
                target.span.clone(),
                sugg,
            )
        })?;

        let target_type = var_info.ty.clone();
        let target_mutability = var_info.mutability;

        if target_mutability != Mutability::Mutable {
            let kind = match target_mutability {
                Mutability::Immutable => "immutable (declared with 'let')",
                Mutability::Constant => "a constant (declared with 'const')",
                Mutability::Mutable => unreachable!(),
            };
            return Err(TypeError::simple(
                format!("cannot assign to '{name}': variable is {kind}"),
                target.span.clone(),
            ));
        }

        let value_type = self.infer_expr(value)?;

        match op {
            AssignOp::Assign => {
                if target_type != value_type {
                    return Err(TypeError::with_suggestions(
                        format!("type mismatch: cannot assign {value_type} to {target_type}"),
                        value.span.clone(),
                        suggestions::suggest_type_mismatch(&value_type, &target_type, &value.span),
                    ));
                }
            }
            AssignOp::AddAssign
            | AssignOp::SubAssign
            | AssignOp::MulAssign
            | AssignOp::DivAssign
            | AssignOp::ModAssign => {
                if !target_type.is_numeric() {
                    return Err(TypeError::simple(
                        format!(
                            "cannot use arithmetic assignment on non-numeric type {target_type}"
                        ),
                        target.span.clone(),
                    ));
                }
                if target_type != value_type {
                    return Err(TypeError::simple(
                        format!(
                            "type mismatch in compound assignment: expected {target_type}, found {value_type}"
                        ),
                        value.span.clone(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_if(
        &mut self,
        condition: &Expr,
        then_block: &[Stmt],
        else_branch: Option<&ElseBranch>,
    ) -> Result<(), TypeError> {
        let cond_type = self.infer_expr(condition)?;
        if cond_type != Type::Bool {
            return Err(TypeError::simple(
                format!("if condition must be bool, found {cond_type}"),
                condition.span.clone(),
            ));
        }

        self.env.push_scope();
        for stmt in then_block {
            self.check_stmt(stmt)?;
        }
        self.env.pop_scope();

        if let Some(branch) = else_branch {
            match branch {
                ElseBranch::ElseBlock(stmts) => {
                    self.env.push_scope();
                    for stmt in stmts {
                        self.check_stmt(stmt)?;
                    }
                    self.env.pop_scope();
                }
                ElseBranch::ElseIf(stmt) => {
                    self.check_stmt(stmt)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_while(&mut self, condition: &Expr, body: &[Stmt]) -> Result<(), TypeError> {
        let cond_type = self.infer_expr(condition)?;
        if cond_type != Type::Bool {
            return Err(TypeError::simple(
                format!("while condition must be bool, found {cond_type}"),
                condition.span.clone(),
            ));
        }

        self.env.push_scope();
        for stmt in body {
            self.check_stmt(stmt)?;
        }
        self.env.pop_scope();
        Ok(())
    }

}
