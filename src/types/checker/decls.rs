use super::*;

impl TypeChecker {
    pub(super) fn check_func_decl(
        &mut self,
        func: &FuncDecl,
        span: &Span,
    ) -> Result<(), TypeError> {
        let return_type = match &func.return_type {
            Some(type_expr) => self.resolve_type_expr(type_expr, span)?,
            None => Type::Void,
        };

        let mut param_types = Vec::new();
        for param in &func.params {
            let ty = self.resolve_type_expr(&param.type_annotation, span)?;
            param_types.push(ty);
        }

        let func_type = Type::Function {
            params: param_types.clone(),
            return_type: Box::new(return_type.clone()),
        };
        self.env.define(
            &func.name,
            VarInfo {
                ty: func_type,
                mutability: Mutability::Immutable,
            },
        );

        let prev_return_type = self.current_return_type.take();
        self.current_return_type = Some(return_type.clone());

        self.env.push_scope();

        // Validate `where` clauses and bind type params to their constraint type in the body scope.
        // This lets the type checker resolve trait method calls on type parameters.
        for clause in &func.where_clauses {
            if self.registry.get_trait(&clause.trait_name).is_none() {
                return Err(TypeError::simple(
                    format!("unknown trait '{}' in where clause", clause.trait_name),
                    span.clone(),
                ));
            }
            // Bind the type param name to its constraint trait type so method calls resolve.
            self.env.define(
                &clause.type_param,
                VarInfo {
                    ty: Type::Trait(clause.trait_name.clone()),
                    mutability: Mutability::Immutable,
                },
            );
        }

        for (param, ty) in func.params.iter().zip(param_types.into_iter()) {
            self.env.define(
                &param.name,
                VarInfo {
                    ty,
                    mutability: Mutability::Immutable,
                },
            );
        }

        for stmt in &func.body {
            self.check_stmt(stmt)?;
        }

        if return_type != Type::Void && !returns_on_all_paths(&func.body) {
            self.env.pop_scope();
            self.current_return_type = prev_return_type;
            return Err(TypeError::simple(
                format!(
                    "missing return on some code paths in function '{}' returning {return_type}",
                    func.name
                ),
                span.clone(),
            ));
        }

        self.env.pop_scope();
        self.current_return_type = prev_return_type;
        Ok(())
    }

    pub(super) fn check_return(
        &mut self,
        value: Option<&Expr>,
        span: &Span,
    ) -> Result<(), TypeError> {
        // At top level (outside any function), current_return_type is None.
        // Allow any return value there.
        let Some(expected) = self.current_return_type.clone() else {
            if let Some(expr) = value {
                self.infer_expr(expr)?;
            }
            return Ok(());
        };

        match value {
            Some(expr) => {
                let actual = self.infer_expr(expr)?;
                if expected == Type::Void {
                    return Err(TypeError::simple(
                        "cannot return a value from a void function".to_string(),
                        expr.span.clone(),
                    ));
                }
                if !self.types_compatible(&expected, &actual) {
                    return Err(TypeError::with_suggestions(
                        format!("return type mismatch: expected {expected}, found {actual}"),
                        expr.span.clone(),
                        suggestions::suggest_type_mismatch(&actual, &expected, &expr.span),
                    ));
                }
                Ok(())
            }
            None => {
                if expected != Type::Void {
                    return Err(TypeError::simple(
                        format!("missing return value: expected {expected}"),
                        span.clone(),
                    ));
                }
                Ok(())
            }
        }
    }

    pub(super) fn check_when(
        &mut self,
        subject: Option<&Expr>,
        arms: &[crate::parser::WhenArm],
        span: &Span,
    ) -> Result<(), TypeError> {
        let Some(subject_expr) = subject else {
            // Subject-less when -- just check each arm body.
            for arm in arms {
                self.env.push_scope();
                self.check_when_body(&arm.body)?;
                self.env.pop_scope();
            }
            return Ok(());
        };

        let subject_type = self.infer_expr(subject_expr)?;

        match &subject_type {
            Type::Result(inner_type) => {
                let mut has_success = false;
                let mut has_error = false;
                let mut has_else = false;

                for arm in arms {
                    self.env.push_scope();
                    match &arm.pattern {
                        WhenPattern::TypeMatch { type_name, binding } => {
                            if type_name == "Success" {
                                has_success = true;
                                if let Some(name) = binding {
                                    self.env.define(
                                        name,
                                        VarInfo {
                                            ty: *inner_type.clone(),
                                            mutability: Mutability::Immutable,
                                        },
                                    );
                                }
                            } else if type_name == "Error" {
                                has_error = true;
                                if let Some(name) = binding {
                                    self.env.define(
                                        name,
                                        VarInfo {
                                            ty: Type::Str,
                                            mutability: Mutability::Immutable,
                                        },
                                    );
                                }
                            }
                        }
                        WhenPattern::Else => has_else = true,
                        _ => {}
                    }
                    self.check_when_body(&arm.body)?;
                    self.env.pop_scope();
                }

                if !(has_else || (has_success && has_error)) {
                    return Err(TypeError::simple("non-exhaustive when over Result<T>: must handle both 'is Success' and 'is Error' (or add 'else')"
                                .to_string(), span.clone()));
                }
                Ok(())
            }
            Type::Enum(enum_name) => {
                let enum_info = self.registry.get_enum(enum_name).ok_or(TypeError::simple(
                    format!("unknown enum '{enum_name}'"),
                    span.clone(),
                ))?;
                let all_variants: Vec<String> = enum_info.variants.clone();
                let mut covered: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                let mut has_else = false;

                for arm in arms {
                    self.env.push_scope();
                    match &arm.pattern {
                        WhenPattern::Value(expr) => {
                            // Match bare identifier (variant name) or qualified EnumName.Variant.
                            let variant_name = match &expr.kind {
                                ExprKind::Identifier(name) => Some(name.clone()),
                                ExprKind::MemberAccess { member, .. } => Some(member.clone()),
                                _ => None,
                            };
                            if let Some(name) = variant_name
                                && all_variants.contains(&name)
                            {
                                covered.insert(name);
                            }
                        }
                        WhenPattern::Else => has_else = true,
                        _ => {}
                    }
                    self.check_when_body(&arm.body)?;
                    self.env.pop_scope();
                }

                if !has_else {
                    let missing: Vec<&str> = all_variants
                        .iter()
                        .filter(|v| !covered.contains(*v))
                        .map(|v| v.as_str())
                        .collect();
                    if !missing.is_empty() {
                        return Err(TypeError::simple(
                            format!(
                                "non-exhaustive when over {enum_name}: missing variant(s) {}",
                                missing.join(", ")
                            ),
                            span.clone(),
                        ));
                    }
                }
                Ok(())
            }
            _ => {
                // For non-Result, non-Enum subjects, just check each arm body.
                for arm in arms {
                    self.env.push_scope();
                    self.check_when_body(&arm.body)?;
                    self.env.pop_scope();
                }
                Ok(())
            }
        }
    }

    pub(super) fn check_when_body(&mut self, body: &WhenBody) -> Result<(), TypeError> {
        match body {
            WhenBody::Expr(e) => {
                self.infer_expr(e)?;
                Ok(())
            }
            WhenBody::Block(stmts) => {
                for s in stmts {
                    self.check_stmt(s)?;
                }
                Ok(())
            }
        }
    }

    pub(super) fn check_let_destructure(
        &mut self,
        names: &[String],
        initializer: &Expr,
        span: &Span,
    ) -> Result<(), TypeError> {
        let init_type = self.infer_expr(initializer)?;
        match init_type {
            Type::Tuple(types) => {
                if names.len() != types.len() {
                    return Err(TypeError::simple(
                        format!(
                            "tuple destructuring: expected {} names, found {}",
                            types.len(),
                            names.len()
                        ),
                        span.clone(),
                    ));
                }
                for (name, ty) in names.iter().zip(types.into_iter()) {
                    self.env.define(
                        name,
                        VarInfo {
                            ty,
                            mutability: Mutability::Immutable,
                        },
                    );
                }
                Ok(())
            }
            _ => Err(TypeError::simple(
                format!("cannot destructure non-tuple type {init_type}"),
                initializer.span.clone(),
            )),
        }
    }

    // --- Phase 7: Class, Trait, Enum checking (pass 2) ---
}
