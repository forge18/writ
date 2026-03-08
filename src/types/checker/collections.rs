use super::*;

impl TypeChecker {
    pub(super) fn infer_constructor_call(
        &mut self,
        class_info: &ClassInfo,
        args: &[CallArg],
        span: &Span,
    ) -> Result<Type, TypeError> {
        let all_fields = self.registry.all_fields(&class_info.name);
        let required_fields: Vec<&FieldInfo> =
            all_fields.iter().filter(|f| !f.has_default).collect();

        // Check if args are named or positional.
        let has_named = args.iter().any(|a| matches!(a, CallArg::Named { .. }));

        if has_named {
            // Named args: match by field name.
            let mut provided: std::collections::HashSet<String> = std::collections::HashSet::new();
            for arg in args {
                match arg {
                    CallArg::Named { name, value } => {
                        let field = all_fields.iter().find(|f| f.name == *name);
                        match field {
                            Some(field_info) => {
                                let actual = self.infer_expr(value)?;
                                if !self.types_compatible(&field_info.ty, &actual) {
                                    return Err(TypeError::simple(
                                        format!(
                                            "constructor field '{name}' type mismatch: expected {}, found {actual}",
                                            field_info.ty
                                        ),
                                        value.span.clone(),
                                    ));
                                }
                                provided.insert(name.clone());
                            }
                            None => {
                                return Err(TypeError::simple(
                                    format!("class '{}' has no field '{name}'", class_info.name),
                                    value.span.clone(),
                                ));
                            }
                        }
                    }
                    CallArg::Positional(expr) => {
                        return Err(TypeError::simple(
                            "cannot mix positional and named arguments in constructor".to_string(),
                            expr.span.clone(),
                        ));
                    }
                }
            }
            // Check all required fields are provided.
            for field in &required_fields {
                if !provided.contains(&field.name) {
                    return Err(TypeError::simple(
                        format!(
                            "missing required field '{}' in constructor for '{}'",
                            field.name, class_info.name
                        ),
                        span.clone(),
                    ));
                }
            }
        } else {
            // Positional args: match in field order.
            // Must provide at least the required fields.
            if args.len() < required_fields.len() || args.len() > all_fields.len() {
                return Err(TypeError::simple(
                    format!(
                        "constructor for '{}' expects {}{} argument(s), found {}",
                        class_info.name,
                        if required_fields.len() == all_fields.len() {
                            String::new()
                        } else {
                            format!("{} to ", required_fields.len())
                        },
                        all_fields.len(),
                        args.len()
                    ),
                    span.clone(),
                ));
            }
            for (i, arg) in args.iter().enumerate() {
                let arg_expr = call_arg_expr(arg);
                let actual = self.infer_expr(arg_expr)?;
                if !self.types_compatible(&all_fields[i].ty, &actual) {
                    return Err(TypeError::simple(
                        format!(
                            "constructor argument {} type mismatch: expected {}, found {actual}",
                            i + 1,
                            all_fields[i].ty
                        ),
                        arg_expr.span.clone(),
                    ));
                }
            }
        }

        Ok(Type::Class(class_info.name.clone()))
    }

    pub(super) fn infer_struct_constructor_call(
        &mut self,
        struct_info: &StructInfo,
        args: &[CallArg],
        span: &Span,
    ) -> Result<Type, TypeError> {
        let all_fields = &struct_info.fields;
        let required_fields: Vec<&FieldInfo> =
            all_fields.iter().filter(|f| !f.has_default).collect();

        let has_named = args.iter().any(|a| matches!(a, CallArg::Named { .. }));

        if has_named {
            let mut provided: std::collections::HashSet<String> = std::collections::HashSet::new();
            for arg in args {
                match arg {
                    CallArg::Named { name, value } => {
                        let field = all_fields.iter().find(|f| f.name == *name);
                        match field {
                            Some(field_info) => {
                                let actual = self.infer_expr(value)?;
                                if !self.types_compatible(&field_info.ty, &actual) {
                                    return Err(TypeError::simple(
                                        format!(
                                            "constructor field '{name}' type mismatch: expected {}, found {actual}",
                                            field_info.ty
                                        ),
                                        value.span.clone(),
                                    ));
                                }
                                provided.insert(name.clone());
                            }
                            None => {
                                return Err(TypeError::simple(
                                    format!("struct '{}' has no field '{name}'", struct_info.name),
                                    value.span.clone(),
                                ));
                            }
                        }
                    }
                    CallArg::Positional(expr) => {
                        return Err(TypeError::simple(
                            "cannot mix positional and named arguments in constructor".to_string(),
                            expr.span.clone(),
                        ));
                    }
                }
            }
            for field in &required_fields {
                if !provided.contains(&field.name) {
                    return Err(TypeError::simple(
                        format!(
                            "missing required field '{}' in constructor for '{}'",
                            field.name, struct_info.name
                        ),
                        span.clone(),
                    ));
                }
            }
        } else {
            if args.len() < required_fields.len() || args.len() > all_fields.len() {
                return Err(TypeError::simple(
                    format!(
                        "constructor for '{}' expects {}{} argument(s), found {}",
                        struct_info.name,
                        if required_fields.len() == all_fields.len() {
                            String::new()
                        } else {
                            format!("{} to ", required_fields.len())
                        },
                        all_fields.len(),
                        args.len()
                    ),
                    span.clone(),
                ));
            }
            for (i, arg) in args.iter().enumerate() {
                let arg_expr = call_arg_expr(arg);
                let actual = self.infer_expr(arg_expr)?;
                if !self.types_compatible(&all_fields[i].ty, &actual) {
                    return Err(TypeError::simple(
                        format!(
                            "constructor argument {} type mismatch: expected {}, found {actual}",
                            i + 1,
                            all_fields[i].ty
                        ),
                        arg_expr.span.clone(),
                    ));
                }
            }
        }

        Ok(Type::Struct(struct_info.name.clone()))
    }

    // ── Collection literal inference ────────────────────────────────────

    pub(super) fn infer_array_literal(
        &mut self,
        elements: &[ArrayElement],
        span: &Span,
    ) -> Result<Type, TypeError> {
        if elements.is_empty() {
            return Ok(Type::Array(Box::new(Type::Unknown)));
        }

        let mut element_type: Option<Type> = None;
        for elem in elements {
            let ty = match elem {
                ArrayElement::Expr(e) => self.infer_expr(e)?,
                ArrayElement::Spread(e) => {
                    let spread_type = self.infer_expr(e)?;
                    match spread_type {
                        Type::Array(inner) => *inner,
                        other => {
                            return Err(TypeError::simple(
                                format!("spread requires Array<T>, found {other}"),
                                span.clone(),
                            ));
                        }
                    }
                }
            };

            match &element_type {
                None => element_type = Some(ty),
                Some(expected) => {
                    if !self.types_compatible(expected, &ty) {
                        return Err(TypeError::simple(
                            format!("array element type mismatch: expected {expected}, found {ty}"),
                            span.clone(),
                        ));
                    }
                }
            }
        }

        Ok(Type::Array(Box::new(element_type.unwrap())))
    }

    pub(super) fn infer_dict_literal(
        &mut self,
        entries: &[DictElement],
        span: &Span,
    ) -> Result<Type, TypeError> {
        if entries.is_empty() {
            return Ok(Type::Dictionary(
                Box::new(Type::Unknown),
                Box::new(Type::Unknown),
            ));
        }

        let mut key_type: Option<Type> = None;
        let mut value_type: Option<Type> = None;
        for entry in entries {
            let (kt, vt) = match entry {
                DictElement::KeyValue { key, value } => {
                    (self.infer_expr(key)?, self.infer_expr(value)?)
                }
                DictElement::Spread(e) => {
                    let spread_type = self.infer_expr(e)?;
                    match spread_type {
                        Type::Dictionary(k, v) => (*k, *v),
                        other => {
                            return Err(TypeError::simple(
                                format!("spread requires Dictionary<K, V>, found {other}"),
                                span.clone(),
                            ));
                        }
                    }
                }
            };

            match &key_type {
                None => key_type = Some(kt),
                Some(expected) => {
                    if !self.types_compatible(expected, &kt) {
                        return Err(TypeError::simple(
                            format!(
                                "dictionary key type mismatch: expected {expected}, found {kt}"
                            ),
                            span.clone(),
                        ));
                    }
                }
            }
            match &value_type {
                None => value_type = Some(vt),
                Some(expected) => {
                    if !self.types_compatible(expected, &vt) {
                        return Err(TypeError::simple(
                            format!(
                                "dictionary value type mismatch: expected {expected}, found {vt}"
                            ),
                            span.clone(),
                        ));
                    }
                }
            }
        }

        Ok(Type::Dictionary(
            Box::new(key_type.unwrap()),
            Box::new(value_type.unwrap()),
        ))
    }

    pub(super) fn infer_namespace_access(
        &self,
        namespace: &str,
        member: &str,
        span: &Span,
    ) -> Result<Type, TypeError> {
        let module_path = self.namespace_aliases.get(namespace).ok_or_else(|| {
            TypeError::simple(format!("unknown namespace '{namespace}'"), span.clone())
        })?;

        self.module_registry
            .get_export(module_path, member)
            .cloned()
            .ok_or_else(|| {
                TypeError::simple(
                    format!("namespace '{namespace}' has no member '{member}'"),
                    span.clone(),
                )
            })
    }

    // ── Module resolution ─────────────────────────────────────────────

    pub(super) fn check_import(
        &mut self,
        import: &ImportDecl,
        span: &Span,
    ) -> Result<(), TypeError> {
        let module = self
            .module_registry
            .get_module(&import.from)
            .ok_or_else(|| {
                TypeError::with_suggestions(
                    format!("unknown module path '{}'", import.from),
                    span.clone(),
                    suggestions::suggest_module_path(&import.from, &self.module_registry, span),
                )
            })?;

        // Clone the exports we need before mutating self.env.
        let mut resolved = Vec::new();
        for name in &import.names {
            let ty = module.get(name).ok_or_else(|| {
                TypeError::with_suggestions(
                    format!("module '{}' has no export '{name}'", import.from),
                    span.clone(),
                    suggestions::suggest_module_exports(&import.from, &self.module_registry, span),
                )
            })?;
            resolved.push((name.clone(), ty.clone()));
        }

        for (name, ty) in resolved {
            self.env.define(
                &name,
                VarInfo {
                    ty,
                    mutability: Mutability::Immutable,
                },
            );
        }
        Ok(())
    }

    pub(super) fn check_wildcard_import(
        &mut self,
        import: &WildcardImportDecl,
        span: &Span,
    ) -> Result<(), TypeError> {
        if self.module_registry.get_module(&import.from).is_none() {
            return Err(TypeError::with_suggestions(
                format!("unknown module path '{}'", import.from),
                span.clone(),
                suggestions::suggest_module_path(&import.from, &self.module_registry, span),
            ));
        }

        self.namespace_aliases
            .insert(import.alias.clone(), import.from.clone());
        Ok(())
    }

    // ── Member access resolution ─────────────────────────────────────

    pub(super) fn resolve_member_access(
        &self,
        obj_type: &Type,
        member: &str,
        span: &Span,
        is_internal: bool,
    ) -> Result<Type, TypeError> {
        match obj_type {
            Type::Class(class_name) => {
                // Search fields (including inherited).
                let all_fields = self.registry.all_fields(class_name);
                if let Some(field) = all_fields.iter().find(|f| f.name == member) {
                    if !is_internal && field.visibility != Visibility::Public {
                        return Err(TypeError::with_suggestions(
                            format!("field '{member}' of class '{class_name}' is private"),
                            span.clone(),
                            suggestions::suggest_public_getter(
                                member,
                                class_name,
                                &self.registry,
                                span,
                            ),
                        ));
                    }
                    return Ok(field.ty.clone());
                }

                // Search methods (including inherited and trait defaults).
                let all_methods = self.registry.all_methods(class_name);
                if let Some(method) = all_methods.iter().find(|m| m.name == member) {
                    if !is_internal && method.visibility != Visibility::Public {
                        return Err(TypeError::simple(
                            format!("method '{member}' of class '{class_name}' is private"),
                            span.clone(),
                        ));
                    }
                    return Ok(Type::Function {
                        params: method.params.clone(),
                        return_type: Box::new(method.return_type.clone()),
                    });
                }

                Err(TypeError::with_suggestions(
                    format!("type '{class_name}' has no member '{member}'"),
                    span.clone(),
                    suggestions::suggest_class_member(member, class_name, &self.registry, span),
                ))
            }
            Type::Enum(enum_name) => {
                let enum_info = match self.registry.get_enum(enum_name) {
                    Some(info) => info,
                    None => {
                        return Err(TypeError::simple(
                            format!("unknown enum '{enum_name}'"),
                            span.clone(),
                        ));
                    }
                };

                // Check variants.
                if enum_info.variants.iter().any(|v| v == member) {
                    return Ok(Type::Enum(enum_name.clone()));
                }

                // Check methods.
                if let Some(method) = enum_info.methods.iter().find(|m| m.name == member) {
                    return Ok(Type::Function {
                        params: method.params.clone(),
                        return_type: Box::new(method.return_type.clone()),
                    });
                }

                Err(TypeError::with_suggestions(
                    format!("enum '{enum_name}' has no member '{member}'"),
                    span.clone(),
                    suggestions::suggest_enum_member(member, enum_name, &self.registry, span),
                ))
            }
            Type::Struct(struct_name) => {
                let struct_info = match self.registry.get_struct(struct_name) {
                    Some(info) => info,
                    None => {
                        return Err(TypeError::simple(
                            format!("unknown struct '{struct_name}'"),
                            span.clone(),
                        ));
                    }
                };

                // Search fields.
                if let Some(field) = struct_info.fields.iter().find(|f| f.name == member) {
                    if !is_internal && field.visibility != Visibility::Public {
                        return Err(TypeError::simple(
                            format!("field '{member}' of struct '{struct_name}' is private"),
                            span.clone(),
                        ));
                    }
                    return Ok(field.ty.clone());
                }

                // Search methods.
                if let Some(method) = struct_info.methods.iter().find(|m| m.name == member) {
                    if !is_internal && method.visibility != Visibility::Public {
                        return Err(TypeError::simple(
                            format!("method '{member}' of struct '{struct_name}' is private"),
                            span.clone(),
                        ));
                    }
                    return Ok(Type::Function {
                        params: method.params.clone(),
                        return_type: Box::new(method.return_type.clone()),
                    });
                }

                Err(TypeError::with_suggestions(
                    format!("struct '{struct_name}' has no member '{member}'"),
                    span.clone(),
                    suggestions::suggest_struct_member(member, struct_name, &self.registry, span),
                ))
            }
            Type::Array(elem) => self.resolve_array_method(elem, member, span),
            Type::Dictionary(k, v) => self.resolve_dict_method(k, v, member, span),
            _ => Err(TypeError::simple(
                format!("cannot access member '{member}' on type '{obj_type}'"),
                span.clone(),
            )),
        }
    }

    pub(super) fn resolve_array_method(
        &self,
        elem: &Type,
        method: &str,
        span: &Span,
    ) -> Result<Type, TypeError> {
        let result = match method {
            "push" => Type::Function {
                params: vec![elem.clone()],
                return_type: Box::new(Type::Void),
            },
            "pop" => Type::Function {
                params: vec![],
                return_type: Box::new(elem.clone()),
            },
            "len" => Type::Function {
                params: vec![],
                return_type: Box::new(Type::Int),
            },
            "isEmpty" => Type::Function {
                params: vec![],
                return_type: Box::new(Type::Bool),
            },
            "contains" => Type::Function {
                params: vec![elem.clone()],
                return_type: Box::new(Type::Bool),
            },
            "indexOf" => Type::Function {
                params: vec![elem.clone()],
                return_type: Box::new(Type::Int),
            },
            "first" | "last" => Type::Function {
                params: vec![],
                return_type: Box::new(Type::Optional(Box::new(elem.clone()))),
            },
            "reverse" | "sort" => Type::Function {
                params: vec![],
                return_type: Box::new(Type::Void),
            },
            "map" => Type::Function {
                params: vec![Type::Function {
                    params: vec![elem.clone()],
                    return_type: Box::new(Type::Unknown),
                }],
                return_type: Box::new(Type::Array(Box::new(Type::Unknown))),
            },
            "filter" => Type::Function {
                params: vec![Type::Function {
                    params: vec![elem.clone()],
                    return_type: Box::new(Type::Bool),
                }],
                return_type: Box::new(Type::Array(Box::new(elem.clone()))),
            },
            "reduce" => Type::Function {
                params: vec![
                    Type::Function {
                        params: vec![Type::Unknown, elem.clone()],
                        return_type: Box::new(Type::Unknown),
                    },
                    Type::Unknown,
                ],
                return_type: Box::new(Type::Unknown),
            },
            "slice" => Type::Function {
                params: vec![Type::Int, Type::Int],
                return_type: Box::new(Type::Array(Box::new(elem.clone()))),
            },
            "insert" => Type::Function {
                params: vec![Type::Int, elem.clone()],
                return_type: Box::new(Type::Void),
            },
            "remove" => Type::Function {
                params: vec![Type::Int],
                return_type: Box::new(elem.clone()),
            },
            _ => {
                return Err(TypeError::simple(
                    format!("Array has no method '{method}'"),
                    span.clone(),
                ));
            }
        };
        Ok(result)
    }

    pub(super) fn resolve_dict_method(
        &self,
        k: &Type,
        v: &Type,
        method: &str,
        span: &Span,
    ) -> Result<Type, TypeError> {
        let result = match method {
            "keys" => Type::Function {
                params: vec![],
                return_type: Box::new(Type::Array(Box::new(k.clone()))),
            },
            "values" => Type::Function {
                params: vec![],
                return_type: Box::new(Type::Array(Box::new(v.clone()))),
            },
            "contains" => Type::Function {
                params: vec![k.clone()],
                return_type: Box::new(Type::Bool),
            },
            "remove" => Type::Function {
                params: vec![k.clone()],
                return_type: Box::new(Type::Void),
            },
            "len" => Type::Function {
                params: vec![],
                return_type: Box::new(Type::Int),
            },
            "isEmpty" => Type::Function {
                params: vec![],
                return_type: Box::new(Type::Bool),
            },
            "merge" => Type::Function {
                params: vec![Type::Dictionary(Box::new(k.clone()), Box::new(v.clone()))],
                return_type: Box::new(Type::Void),
            },
            _ => {
                return Err(TypeError::simple(
                    format!("Dictionary has no method '{method}'"),
                    span.clone(),
                ));
            }
        };
        Ok(result)
    }
}
