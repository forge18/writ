use super::*;

impl TypeChecker {
    pub(super) fn check_trait_decl(
        &mut self,
        decl: &TraitDecl,
        span: &Span,
    ) -> Result<(), TypeError> {
        // Trait was already registered in pass 1. Now type-check default bodies.
        for method in &decl.methods {
            if let Some(body) = &method.default_body {
                self.env.push_scope();
                // Define `self` inside method scope.
                self.env.define(
                    "self",
                    VarInfo {
                        ty: Type::Trait(decl.name.clone()),
                        mutability: Mutability::Mutable,
                    },
                );
                for param in &method.params {
                    let ty = self.resolve_type_expr(&param.type_annotation, span)?;
                    self.env.define(
                        &param.name,
                        VarInfo {
                            ty,
                            mutability: Mutability::Immutable,
                        },
                    );
                }
                let return_type = match &method.return_type {
                    Some(te) => self.resolve_type_expr(te, span)?,
                    None => Type::Void,
                };
                let prev_return_type = self.current_return_type.take();
                self.current_return_type = Some(return_type.clone());
                for stmt in body {
                    self.check_stmt(stmt)?;
                }
                if return_type != Type::Void && !returns_on_all_paths(body) {
                    self.env.pop_scope();
                    self.current_return_type = prev_return_type;
                    return Err(TypeError::simple(
                        format!(
                            "missing return on some code paths in default method '{}' returning {return_type}",
                            method.name
                        ),
                        span.clone(),
                    ));
                }
                self.env.pop_scope();
                self.current_return_type = prev_return_type;
            }
        }
        Ok(())
    }

    pub(super) fn check_class_decl(
        &mut self,
        decl: &ClassDecl,
        span: &Span,
    ) -> Result<(), TypeError> {
        // Generic templates are checked only when instantiated.
        if !decl.type_params.is_empty() {
            return Ok(());
        }

        let class_name = &decl.name;

        // Verify parent exists if extends is specified (checked in pass 2 to allow forward refs).
        if let Some(parent) = &decl.extends
            && self.registry.get_class(parent).is_none()
        {
            return Err(TypeError::simple(
                format!("unknown parent class '{parent}'"),
                span.clone(),
            ));
        }

        // Validate field defaults match their type annotations.
        for field in &decl.fields {
            if let Some(default_expr) = &field.default {
                let field_type = self.resolve_type_expr(&field.type_annotation, span)?;
                let default_type = self.infer_expr(default_expr)?;
                if !self.types_compatible(&field_type, &default_type) {
                    return Err(TypeError::simple(
                        format!(
                            "field '{}' type mismatch: expected {field_type}, found {default_type}",
                            field.name
                        ),
                        default_expr.span.clone(),
                    ));
                }
            }
        }

        // Type-check methods.
        let prev_class = self.current_class.take();
        self.current_class = Some(class_name.clone());
        for method in &decl.methods {
            self.env.push_scope();

            // Define `self` inside method scope.
            if !method.is_static {
                self.env.define(
                    "self",
                    VarInfo {
                        ty: Type::Class(class_name.clone()),
                        mutability: Mutability::Mutable,
                    },
                );

                // Define all class fields as accessible variables (unqualified access
                // per spec section 6.3).
                let all_fields = self.registry.all_fields(class_name);
                for field in &all_fields {
                    self.env.define(
                        &field.name,
                        VarInfo {
                            ty: field.ty.clone(),
                            mutability: Mutability::Mutable,
                        },
                    );
                }
            }

            // Define method parameters.
            let return_type = match &method.return_type {
                Some(te) => self.resolve_type_expr(te, span)?,
                None => Type::Void,
            };
            for param in &method.params {
                let ty = self.resolve_type_expr(&param.type_annotation, span)?;
                self.env.define(
                    &param.name,
                    VarInfo {
                        ty,
                        mutability: Mutability::Immutable,
                    },
                );
            }

            let prev_return_type = self.current_return_type.take();
            self.current_return_type = Some(return_type.clone());

            for stmt in &method.body {
                self.check_stmt(stmt)?;
            }

            if return_type != Type::Void && !returns_on_all_paths(&method.body) {
                self.env.pop_scope();
                self.current_return_type = prev_return_type;
                return Err(TypeError::simple(
                    format!(
                        "missing return on some code paths in method '{}' returning {return_type}",
                        method.name
                    ),
                    span.clone(),
                ));
            }

            self.env.pop_scope();
            self.current_return_type = prev_return_type;
        }
        self.current_class = prev_class;

        // Validate trait implementations.
        self.validate_trait_implementations(decl, span)?;

        // Type-check setters.
        for field in &decl.fields {
            if let Some(setter) = &field.setter {
                let field_type = self.resolve_type_expr(&field.type_annotation, span)?;
                self.env.push_scope();
                self.env.define(
                    &setter.param_name,
                    VarInfo {
                        ty: field_type.clone(),
                        mutability: Mutability::Immutable,
                    },
                );
                self.env.define(
                    "field",
                    VarInfo {
                        ty: field_type,
                        mutability: Mutability::Mutable,
                    },
                );
                self.env.define(
                    "self",
                    VarInfo {
                        ty: Type::Class(class_name.clone()),
                        mutability: Mutability::Mutable,
                    },
                );
                for stmt in &setter.body {
                    self.check_stmt(stmt)?;
                }
                self.env.pop_scope();
            }
        }

        Ok(())
    }

    pub(super) fn validate_trait_implementations(
        &self,
        decl: &ClassDecl,
        span: &Span,
    ) -> Result<(), TypeError> {
        let class_info = self
            .registry
            .get_class(&decl.name)
            .expect("class should be registered in pass 1");

        // Collect all default method names from all traits (for conflict detection).
        let mut default_method_sources: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        for trait_name in &decl.traits {
            let trait_info = match self.registry.get_trait(trait_name) {
                Some(info) => info,
                None => {
                    return Err(TypeError::simple(
                        format!("unknown trait '{trait_name}'"),
                        span.clone(),
                    ));
                }
            };

            for trait_method in &trait_info.methods {
                // Check if the class implements this method.
                let class_has_method = class_info
                    .methods
                    .iter()
                    .any(|m| m.name == trait_method.name);

                if !class_has_method && !trait_method.has_default_body {
                    return Err(TypeError::simple(
                        format!(
                            "class '{}' does not implement required method '{}' from trait '{trait_name}'",
                            decl.name, trait_method.name
                        ),
                        span.clone(),
                    ));
                }

                // Check signature match if the class provides an implementation.
                if class_has_method {
                    let class_method = class_info
                        .methods
                        .iter()
                        .find(|m| m.name == trait_method.name)
                        .unwrap();
                    if class_method.params != trait_method.params
                        || class_method.return_type != trait_method.return_type
                    {
                        return Err(TypeError::simple(
                            format!(
                                "method '{}' in class '{}' has a different signature than trait '{trait_name}' requires",
                                trait_method.name, decl.name
                            ),
                            span.clone(),
                        ));
                    }
                }

                // Track default method sources for conflict detection.
                if trait_method.has_default_body && !class_has_method {
                    default_method_sources
                        .entry(trait_method.name.clone())
                        .or_default()
                        .push(trait_name.clone());
                }
            }
        }

        // Detect conflicts: same default method name from multiple traits without class override.
        for (method_name, sources) in &default_method_sources {
            if sources.len() > 1 {
                return Err(TypeError::simple(
                    format!(
                        "conflicting default method '{method_name}' from traits {}; class '{}' must provide an override",
                        sources.join(" and "),
                        decl.name
                    ),
                    span.clone(),
                ));
            }
        }

        Ok(())
    }

    pub(super) fn check_struct_decl(
        &mut self,
        decl: &StructDecl,
        span: &Span,
    ) -> Result<(), TypeError> {
        // Generic templates are checked only when instantiated.
        if !decl.type_params.is_empty() {
            return Ok(());
        }

        let struct_name = &decl.name;

        // Validate field defaults match their type annotations.
        for field in &decl.fields {
            if let Some(default_expr) = &field.default {
                let field_type = self.resolve_type_expr(&field.type_annotation, span)?;
                let default_type = self.infer_expr(default_expr)?;
                if !self.types_compatible(&field_type, &default_type) {
                    return Err(TypeError::simple(
                        format!(
                            "field '{}' type mismatch: expected {field_type}, found {default_type}",
                            field.name
                        ),
                        default_expr.span.clone(),
                    ));
                }
            }
        }

        // Type-check methods.
        for method in &decl.methods {
            self.env.push_scope();

            // Define `self` inside method scope.
            if !method.is_static {
                self.env.define(
                    "self",
                    VarInfo {
                        ty: Type::Struct(struct_name.clone()),
                        mutability: Mutability::Mutable,
                    },
                );

                // Define struct fields as accessible variables (unqualified access).
                let struct_info = self
                    .registry
                    .get_struct(struct_name)
                    .expect("struct should be registered in pass 1");
                for field in &struct_info.fields {
                    self.env.define(
                        &field.name,
                        VarInfo {
                            ty: field.ty.clone(),
                            mutability: Mutability::Mutable,
                        },
                    );
                }
            }

            // Define method parameters.
            let return_type = match &method.return_type {
                Some(te) => self.resolve_type_expr(te, span)?,
                None => Type::Void,
            };
            for param in &method.params {
                let ty = self.resolve_type_expr(&param.type_annotation, span)?;
                self.env.define(
                    &param.name,
                    VarInfo {
                        ty,
                        mutability: Mutability::Immutable,
                    },
                );
            }

            let prev_return_type = self.current_return_type.take();
            self.current_return_type = Some(return_type.clone());

            for stmt in &method.body {
                self.check_stmt(stmt)?;
            }

            if return_type != Type::Void && !returns_on_all_paths(&method.body) {
                self.env.pop_scope();
                self.current_return_type = prev_return_type;
                return Err(TypeError::simple(
                    format!(
                        "missing return on some code paths in method '{}' returning {return_type}",
                        method.name
                    ),
                    span.clone(),
                ));
            }

            self.env.pop_scope();
            self.current_return_type = prev_return_type;
        }

        // Type-check setters.
        for field in &decl.fields {
            if let Some(setter) = &field.setter {
                let field_type = self.resolve_type_expr(&field.type_annotation, span)?;
                self.env.push_scope();
                self.env.define(
                    &setter.param_name,
                    VarInfo {
                        ty: field_type.clone(),
                        mutability: Mutability::Immutable,
                    },
                );
                self.env.define(
                    "field",
                    VarInfo {
                        ty: field_type,
                        mutability: Mutability::Mutable,
                    },
                );
                self.env.define(
                    "self",
                    VarInfo {
                        ty: Type::Struct(struct_name.clone()),
                        mutability: Mutability::Mutable,
                    },
                );
                for stmt in &setter.body {
                    self.check_stmt(stmt)?;
                }
                self.env.pop_scope();
            }
        }

        Ok(())
    }

    pub(super) fn check_enum_decl(
        &mut self,
        decl: &EnumDecl,
        span: &Span,
    ) -> Result<(), TypeError> {
        // Validate variant values if present.
        for variant in &decl.variants {
            if let Some(value_expr) = &variant.value {
                self.infer_expr(value_expr)?;
            }
        }

        // Type-check field defaults.
        for field in &decl.fields {
            if let Some(default_expr) = &field.default {
                let field_type = self.resolve_type_expr(&field.type_annotation, span)?;
                let default_type = self.infer_expr(default_expr)?;
                if !self.types_compatible(&field_type, &default_type) {
                    return Err(TypeError::simple(
                        format!(
                            "enum field '{}' type mismatch: expected {field_type}, found {default_type}",
                            field.name
                        ),
                        default_expr.span.clone(),
                    ));
                }
            }
        }

        // Type-check methods.
        for method in &decl.methods {
            self.env.push_scope();
            if !method.is_static {
                self.env.define(
                    "self",
                    VarInfo {
                        ty: Type::Enum(decl.name.clone()),
                        mutability: Mutability::Mutable,
                    },
                );
                // Define enum fields in method scope.
                let enum_info = self
                    .registry
                    .get_enum(&decl.name)
                    .expect("enum should be registered in pass 1");
                for field in &enum_info.fields {
                    self.env.define(
                        &field.name,
                        VarInfo {
                            ty: field.ty.clone(),
                            mutability: Mutability::Mutable,
                        },
                    );
                }
            }

            let return_type = match &method.return_type {
                Some(te) => self.resolve_type_expr(te, span)?,
                None => Type::Void,
            };
            for param in &method.params {
                let ty = self.resolve_type_expr(&param.type_annotation, span)?;
                self.env.define(
                    &param.name,
                    VarInfo {
                        ty,
                        mutability: Mutability::Immutable,
                    },
                );
            }

            let prev_return_type = self.current_return_type.take();
            self.current_return_type = Some(return_type.clone());
            for stmt in &method.body {
                self.check_stmt(stmt)?;
            }
            if return_type != Type::Void && !returns_on_all_paths(&method.body) {
                self.env.pop_scope();
                self.current_return_type = prev_return_type;
                return Err(TypeError::simple(
                    format!(
                        "missing return on some code paths in enum method '{}' returning {return_type}",
                        method.name
                    ),
                    span.clone(),
                ));
            }
            self.env.pop_scope();
            self.current_return_type = prev_return_type;
        }

        Ok(())
    }
}
