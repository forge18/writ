use super::*;

impl TypeChecker {
    pub(super) fn resolve_type_expr(
        &mut self,
        type_expr: &TypeExpr,
        span: &Span,
    ) -> Result<Type, TypeError> {
        match type_expr {
            TypeExpr::Simple(name) => match name.as_str() {
                "int" => Ok(Type::Int),
                "float" => Ok(Type::Float),
                "bool" => Ok(Type::Bool),
                "string" => Ok(Type::Str),
                "void" => Ok(Type::Void),
                other => {
                    if self.registry.get_class(other).is_some() {
                        Ok(Type::Class(other.to_string()))
                    } else if self.registry.get_trait(other).is_some() {
                        Ok(Type::Trait(other.to_string()))
                    } else if self.registry.get_enum(other).is_some() {
                        Ok(Type::Enum(other.to_string()))
                    } else if self.registry.get_struct(other).is_some() {
                        Ok(Type::Struct(other.to_string()))
                    } else {
                        Err(TypeError::with_suggestions(
                            format!("unknown type '{other}'"),
                            span.clone(),
                            suggestions::suggest_type_name(other, &self.registry, span),
                        ))
                    }
                }
            },
            TypeExpr::Generic { name, args } => match name.as_str() {
                "Result" => {
                    if args.len() != 1 {
                        return Err(TypeError::simple(
                            "Result expects exactly one type argument".to_string(),
                            span.clone(),
                        ));
                    }
                    let inner = self.resolve_type_expr(&args[0], span)?;
                    Ok(Type::Result(Box::new(inner)))
                }
                "Optional" => {
                    if args.len() != 1 {
                        return Err(TypeError::simple(
                            "Optional expects exactly one type argument".to_string(),
                            span.clone(),
                        ));
                    }
                    let inner = self.resolve_type_expr(&args[0], span)?;
                    Ok(Type::Optional(Box::new(inner)))
                }
                "Array" => {
                    if args.len() != 1 {
                        return Err(TypeError::simple(
                            "Array expects exactly one type argument".to_string(),
                            span.clone(),
                        ));
                    }
                    let inner = self.resolve_type_expr(&args[0], span)?;
                    Ok(Type::Array(Box::new(inner)))
                }
                "Dictionary" => {
                    if args.len() != 2 {
                        return Err(TypeError::simple(
                            "Dictionary expects exactly two type arguments".to_string(),
                            span.clone(),
                        ));
                    }
                    let k = self.resolve_type_expr(&args[0], span)?;
                    let v = self.resolve_type_expr(&args[1], span)?;
                    Ok(Type::Dictionary(Box::new(k), Box::new(v)))
                }
                _ => {
                    // Try user-defined generic class or struct.
                    self.instantiate_generic(name, args, span)
                }
            },
            TypeExpr::Tuple(types) => {
                let resolved: Vec<Type> = types
                    .iter()
                    .map(|t| self.resolve_type_expr(t, span))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Type::Tuple(resolved))
            }
            TypeExpr::Qualified { namespace, name } => {
                let module_path = self.namespace_aliases.get(namespace).ok_or_else(|| {
                    TypeError::simple(format!("unknown namespace '{namespace}'"), span.clone())
                })?;
                self.module_registry
                    .get_export(module_path, name)
                    .cloned()
                    .ok_or_else(|| {
                        TypeError::simple(
                            format!("namespace '{namespace}' has no member '{name}'"),
                            span.clone(),
                        )
                    })
            }
        }
    }

    /// Produces a synthetic monomorphic name for a generic instantiation.
    /// e.g. `Stack<int>` -> `"Stack__int"`, `Pair<string, int>` -> `"Pair__string__int"`.
    pub(super) fn monomorphic_name(base: &str, arg_types: &[Type]) -> String {
        let mut name = base.to_string();
        for ty in arg_types {
            name.push_str("__");
            name.push_str(&ty.to_string());
        }
        name
    }

    /// Substitutes type-parameter references in a `TypeExpr` given a binding map.
    /// e.g. if bindings = {"T": "int"}, `Simple("T")` becomes `Simple("int")`.
    pub(super) fn substitute_type_expr(
        type_expr: &TypeExpr,
        bindings: &HashMap<String, String>,
    ) -> TypeExpr {
        match type_expr {
            TypeExpr::Simple(name) => {
                if let Some(bound) = bindings.get(name) {
                    TypeExpr::Simple(bound.clone())
                } else {
                    TypeExpr::Simple(name.clone())
                }
            }
            TypeExpr::Generic { name, args } => TypeExpr::Generic {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|a| Self::substitute_type_expr(a, bindings))
                    .collect(),
            },
            TypeExpr::Tuple(types) => TypeExpr::Tuple(
                types
                    .iter()
                    .map(|t| Self::substitute_type_expr(t, bindings))
                    .collect(),
            ),
            TypeExpr::Qualified { namespace, name } => TypeExpr::Qualified {
                namespace: namespace.clone(),
                name: name.clone(),
            },
        }
    }

    /// Instantiates a user-defined generic class or struct for the given type arguments.
    /// Registers the concrete monomorphic type in the registry and returns its `Type`.
    pub(super) fn instantiate_generic(
        &mut self,
        name: &str,
        args: &[TypeExpr],
        span: &Span,
    ) -> Result<Type, TypeError> {
        // Resolve the type arguments first.
        let mut arg_types = Vec::with_capacity(args.len());
        for arg in args {
            arg_types.push(self.resolve_type_expr(arg, span)?);
        }

        let mono_name = Self::monomorphic_name(name, &arg_types);

        // If already instantiated, return cached type.
        if self.registry.get_class(&mono_name).is_some() {
            return Ok(Type::Class(mono_name));
        }
        if self.registry.get_struct(&mono_name).is_some() {
            return Ok(Type::Struct(mono_name));
        }

        // Look up the template.
        if let Some(template) = self.generic_classes.get(name).cloned() {
            if template.type_params.len() != arg_types.len() {
                return Err(TypeError::simple(
                    format!(
                        "'{}' expects {} type argument(s), got {}",
                        name,
                        template.type_params.len(),
                        arg_types.len()
                    ),
                    span.clone(),
                ));
            }
            // Build substitution map: param name -> concrete type display string.
            let bindings: HashMap<String, String> = template
                .type_params
                .iter()
                .zip(arg_types.iter())
                .map(|(p, t)| (p.clone(), t.to_string()))
                .collect();

            // Substitute fields.
            let mut fields = Vec::new();
            for field in &template.fields {
                let subst = Self::substitute_type_expr(&field.type_annotation, &bindings);
                let ty = self.resolve_type_expr(&subst, span)?;
                fields.push(FieldInfo {
                    name: field.name.clone(),
                    ty,
                    visibility: field.visibility,
                    has_default: field.default.is_some(),
                    has_setter: field.setter.is_some(),
                });
            }

            // Substitute methods.
            let mut methods = Vec::new();
            for method in &template.methods {
                let return_type = match &method.return_type {
                    Some(te) => {
                        let subst = Self::substitute_type_expr(te, &bindings);
                        self.resolve_type_expr(&subst, span)?
                    }
                    None => Type::Void,
                };
                let mut params = Vec::new();
                for param in &method.params {
                    let subst = Self::substitute_type_expr(&param.type_annotation, &bindings);
                    params.push(self.resolve_type_expr(&subst, span)?);
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

            let constructor_params: Vec<Type> = fields.iter().map(|f| f.ty.clone()).collect();
            self.registry.register_class(ClassInfo {
                name: mono_name.clone(),
                fields,
                methods,
                parent: None,
                traits: Vec::new(),
            });
            self.env.define(
                &mono_name,
                VarInfo {
                    ty: Type::Function {
                        params: constructor_params,
                        return_type: Box::new(Type::Class(mono_name.clone())),
                    },
                    mutability: Mutability::Immutable,
                },
            );
            return Ok(Type::Class(mono_name));
        }

        if let Some(template) = self.generic_structs.get(name).cloned() {
            if template.type_params.len() != arg_types.len() {
                return Err(TypeError::simple(
                    format!(
                        "'{}' expects {} type argument(s), got {}",
                        name,
                        template.type_params.len(),
                        arg_types.len()
                    ),
                    span.clone(),
                ));
            }
            let bindings: HashMap<String, String> = template
                .type_params
                .iter()
                .zip(arg_types.iter())
                .map(|(p, t)| (p.clone(), t.to_string()))
                .collect();

            let mut fields = Vec::new();
            for field in &template.fields {
                let subst = Self::substitute_type_expr(&field.type_annotation, &bindings);
                let ty = self.resolve_type_expr(&subst, span)?;
                fields.push(FieldInfo {
                    name: field.name.clone(),
                    ty,
                    visibility: field.visibility,
                    has_default: field.default.is_some(),
                    has_setter: field.setter.is_some(),
                });
            }

            let mut methods = Vec::new();
            for method in &template.methods {
                let return_type = match &method.return_type {
                    Some(te) => {
                        let subst = Self::substitute_type_expr(te, &bindings);
                        self.resolve_type_expr(&subst, span)?
                    }
                    None => Type::Void,
                };
                let mut params = Vec::new();
                for param in &method.params {
                    let subst = Self::substitute_type_expr(&param.type_annotation, &bindings);
                    params.push(self.resolve_type_expr(&subst, span)?);
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

            let constructor_params: Vec<Type> = fields.iter().map(|f| f.ty.clone()).collect();
            self.registry.register_struct(StructInfo {
                name: mono_name.clone(),
                fields,
                methods,
            });
            self.env.define(
                &mono_name,
                VarInfo {
                    ty: Type::Function {
                        params: constructor_params,
                        return_type: Box::new(Type::Struct(mono_name.clone())),
                    },
                    mutability: Mutability::Immutable,
                },
            );
            return Ok(Type::Struct(mono_name));
        }

        Err(TypeError::simple(
            format!("unknown generic type '{name}'"),
            span.clone(),
        ))
    }

    /// Checks if `actual` is compatible with `expected`.
    ///
    /// This allows `Optional<Unknown>` (from `null`) to match any `Optional<T>`,
    /// `Result<Unknown>` (from `Error(msg)`) to match any `Result<T>`,
    /// child classes to be assigned where parent is expected, and classes
    /// to be assigned where an implemented trait is expected.
    pub(super) fn types_compatible(&self, expected: &Type, actual: &Type) -> bool {
        if expected == actual {
            return true;
        }
        match (expected, actual) {
            (Type::Optional(_), Type::Optional(inner)) if **inner == Type::Unknown => true,
            (Type::Result(_), Type::Result(inner)) if **inner == Type::Unknown => true,
            (Type::Class(parent), Type::Class(child)) => self.registry.is_subclass(child, parent),
            (Type::Trait(trait_name), Type::Class(class_name)) => self
                .registry
                .get_class(class_name)
                .map(|info| info.traits.contains(trait_name))
                .unwrap_or(false),
            // Function compatibility: param counts must match, each param and return
            // must be compatible. Unknown in expected position accepts any concrete type.
            (
                Type::Function {
                    params: expected_params,
                    return_type: expected_ret,
                },
                Type::Function {
                    params: actual_params,
                    return_type: actual_ret,
                },
            ) => {
                if expected_params.len() != actual_params.len() {
                    return false;
                }
                let params_ok = expected_params
                    .iter()
                    .zip(actual_params.iter())
                    .all(|(exp, act)| *exp == Type::Unknown || self.types_compatible(exp, act));
                let ret_ok = **expected_ret == Type::Unknown
                    || self.types_compatible(expected_ret, actual_ret);
                params_ok && ret_ok
            }
            // Empty array `[]` (Array<Unknown>) is assignable to any Array<T>.
            (Type::Array(_), Type::Array(inner)) if **inner == Type::Unknown => true,
            // Empty dict `{}` (Dictionary<Unknown, Unknown>) assignable to any Dictionary<K, V>.
            (Type::Dictionary(_, _), Type::Dictionary(k, v))
                if **k == Type::Unknown && **v == Type::Unknown =>
            {
                true
            }
            _ => false,
        }
    }
}
