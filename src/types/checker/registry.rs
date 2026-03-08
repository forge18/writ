use super::*;

impl TypeChecker {
    pub(super) fn register_type_if_decl(&mut self, stmt: &Stmt) -> Result<(), TypeError> {
        match &stmt.kind {
            StmtKind::Trait(decl) => self.register_trait(decl, &stmt.span),
            StmtKind::Class(decl) => self.register_class(decl, &stmt.span),
            StmtKind::Enum(decl) => self.register_enum(decl, &stmt.span),
            StmtKind::Struct(decl) => self.register_struct(decl, &stmt.span),
            StmtKind::Func(func) => {
                // Pre-register function signatures so they can be called from
                // class/trait/enum method bodies.
                self.register_func_signature(func, &stmt.span)
            }
            // Process imports in pass 1 so imported types are available for
            // class field annotations and other forward references.
            StmtKind::Import(import) => self.check_import(import, &stmt.span),
            StmtKind::WildcardImport(import) => self.check_wildcard_import(import, &stmt.span),
            // Unwrap exports and recurse to register the inner declaration.
            StmtKind::Export(inner) => self.register_type_if_decl(inner),
            _ => Ok(()),
        }
    }

    pub(super) fn register_func_signature(&mut self, func: &FuncDecl, span: &Span) -> Result<(), TypeError> {
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
            params: param_types,
            return_type: Box::new(return_type),
        };
        self.env.define(
            &func.name,
            VarInfo {
                ty: func_type,
                mutability: Mutability::Immutable,
            },
        );
        Ok(())
    }

    pub(super) fn register_trait(&mut self, decl: &TraitDecl, span: &Span) -> Result<(), TypeError> {
        let mut methods = Vec::new();
        for method in &decl.methods {
            let return_type = match &method.return_type {
                Some(te) => self.resolve_type_expr(te, span)?,
                None => Type::Void,
            };
            let mut params = Vec::new();
            for param in &method.params {
                params.push(self.resolve_type_expr(&param.type_annotation, span)?);
            }
            methods.push(MethodInfo {
                name: method.name.clone(),
                params,
                return_type,
                is_static: false,
                visibility: Visibility::Public,
                has_default_body: method.default_body.is_some(),
            });
        }
        self.registry.register_trait(TraitInfo {
            name: decl.name.clone(),
            methods,
        });
        Ok(())
    }

    pub(super) fn register_class(&mut self, decl: &ClassDecl, span: &Span) -> Result<(), TypeError> {
        // Generic templates are not registered as concrete types — stored for later instantiation.
        if !decl.type_params.is_empty() {
            self.generic_classes.insert(decl.name.clone(), decl.clone());
            return Ok(());
        }

        let mut fields = Vec::new();
        for field in &decl.fields {
            let ty = self.resolve_type_expr(&field.type_annotation, span)?;
            fields.push(FieldInfo {
                name: field.name.clone(),
                ty,
                visibility: field.visibility,
                has_default: field.default.is_some(),
                has_setter: field.setter.is_some(),
            });
        }

        let mut methods = Vec::new();
        for method in &decl.methods {
            let return_type = match &method.return_type {
                Some(te) => self.resolve_type_expr(te, span)?,
                None => Type::Void,
            };
            let mut params = Vec::new();
            for param in &method.params {
                params.push(self.resolve_type_expr(&param.type_annotation, span)?);
            }
            methods.push(MethodInfo {
                name: method.name.clone(),
                params,
                return_type,
                is_static: method.is_static,
                visibility: method.visibility,
                has_default_body: false,
            });
        }

        self.registry.register_class(ClassInfo {
            name: decl.name.clone(),
            fields: fields.clone(),
            methods,
            parent: decl.extends.clone(),
            traits: decl.traits.clone(),
        });

        // Register auto-generated constructor in the environment.
        let constructor_params: Vec<Type> = fields.iter().map(|f| f.ty.clone()).collect();
        self.env.define(
            &decl.name,
            VarInfo {
                ty: Type::Function {
                    params: constructor_params,
                    return_type: Box::new(Type::Class(decl.name.clone())),
                },
                mutability: Mutability::Immutable,
            },
        );

        Ok(())
    }

    pub(super) fn register_struct(&mut self, decl: &StructDecl, span: &Span) -> Result<(), TypeError> {
        // Generic templates are not registered as concrete types — stored for later instantiation.
        if !decl.type_params.is_empty() {
            self.generic_structs.insert(decl.name.clone(), decl.clone());
            return Ok(());
        }

        let mut fields = Vec::new();
        for field in &decl.fields {
            let ty = self.resolve_type_expr(&field.type_annotation, span)?;
            fields.push(FieldInfo {
                name: field.name.clone(),
                ty,
                visibility: field.visibility,
                has_default: field.default.is_some(),
                has_setter: field.setter.is_some(),
            });
        }

        let mut methods = Vec::new();
        for method in &decl.methods {
            let return_type = match &method.return_type {
                Some(te) => self.resolve_type_expr(te, span)?,
                None => Type::Void,
            };
            let mut params = Vec::new();
            for param in &method.params {
                params.push(self.resolve_type_expr(&param.type_annotation, span)?);
            }
            methods.push(MethodInfo {
                name: method.name.clone(),
                params,
                return_type,
                is_static: method.is_static,
                visibility: method.visibility,
                has_default_body: false,
            });
        }

        self.registry.register_struct(StructInfo {
            name: decl.name.clone(),
            fields: fields.clone(),
            methods,
        });

        // Register auto-generated constructor in the environment.
        let constructor_params: Vec<Type> = fields.iter().map(|f| f.ty.clone()).collect();
        self.env.define(
            &decl.name,
            VarInfo {
                ty: Type::Function {
                    params: constructor_params,
                    return_type: Box::new(Type::Struct(decl.name.clone())),
                },
                mutability: Mutability::Immutable,
            },
        );

        Ok(())
    }

    pub(super) fn register_enum(&mut self, decl: &EnumDecl, span: &Span) -> Result<(), TypeError> {
        let variants: Vec<String> = decl.variants.iter().map(|v| v.name.clone()).collect();

        let mut fields = Vec::new();
        for field in &decl.fields {
            let ty = self.resolve_type_expr(&field.type_annotation, span)?;
            fields.push(FieldInfo {
                name: field.name.clone(),
                ty,
                visibility: field.visibility,
                has_default: field.default.is_some(),
                has_setter: field.setter.is_some(),
            });
        }

        let mut methods = Vec::new();
        for method in &decl.methods {
            let return_type = match &method.return_type {
                Some(te) => self.resolve_type_expr(te, span)?,
                None => Type::Void,
            };
            let mut params = Vec::new();
            for param in &method.params {
                params.push(self.resolve_type_expr(&param.type_annotation, span)?);
            }
            methods.push(MethodInfo {
                name: method.name.clone(),
                params,
                return_type,
                is_static: method.is_static,
                visibility: method.visibility,
                has_default_body: false,
            });
        }

        self.registry.register_enum(EnumInfo {
            name: decl.name.clone(),
            variants,
            fields,
            methods,
        });

        // Register enum name so Direction.North resolves via MemberAccess.
        self.env.define(
            &decl.name,
            VarInfo {
                ty: Type::Enum(decl.name.clone()),
                mutability: Mutability::Immutable,
            },
        );

        Ok(())
    }
}


