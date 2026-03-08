use super::*;

impl TypeChecker {
    pub fn infer_expr(&mut self, expr: &Expr) -> Result<Type, TypeError> {
        match &expr.kind {
            ExprKind::Literal(lit) => Ok(infer_literal(lit)),

            ExprKind::Identifier(name) => self
                .env
                .lookup(name)
                .map(|info| info.ty.clone())
                .ok_or_else(|| {
                    let sugg =
                        suggestions::suggest_variable_weighted(name, &self.env, None, &expr.span);
                    TypeError::with_suggestions(
                        format!("undefined variable '{name}'"),
                        expr.span.clone(),
                        sugg,
                    )
                }),

            ExprKind::Binary { op, lhs, rhs } => self.infer_binary(op, lhs, rhs, &expr.span),

            ExprKind::Unary { op, operand } => self.infer_unary(op, operand),

            ExprKind::Grouped(inner) => self.infer_expr(inner),

            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => self.infer_ternary(condition, then_expr, else_expr, &expr.span),

            ExprKind::StringInterpolation(segments) => {
                for segment in segments {
                    if let InterpolationSegment::Expression(inner) = segment {
                        self.infer_expr(inner)?;
                    }
                }
                Ok(Type::Str)
            }

            ExprKind::Range {
                start,
                end,
                inclusive,
            } => {
                let start_type = self.infer_expr(start)?;
                let end_type = self.infer_expr(end)?;
                // `..` between strings is concatenation
                if start_type == Type::Str && end_type == Type::Str {
                    if *inclusive {
                        return Err(TypeError::simple(
                            "string concatenation uses `..`, not `..=`".to_string(),
                            expr.span.clone(),
                        ));
                    }
                    Ok(Type::Str)
                } else if start_type.is_numeric() && end_type.is_numeric() {
                    // Numeric ranges (used in for..in and when patterns)
                    Ok(Type::Array(Box::new(start_type)))
                } else {
                    Err(TypeError::simple(
                        format!(
                            "`..` requires both operands to be strings (concatenation) or numeric (range), found {start_type} and {end_type}"
                        ),
                        expr.span.clone(),
                    ))
                }
            }

            ExprKind::NullCoalesce { lhs, rhs } => self.infer_null_coalesce(lhs, rhs, &expr.span),

            ExprKind::SafeAccess { object, member } => {
                let obj_type = self.infer_expr(object)?;
                match &obj_type {
                    Type::Optional(inner) => match inner.as_ref() {
                        Type::Class(_) | Type::Enum(_) => {
                            let member_type =
                                self.resolve_member_access(inner, member, &expr.span, false)?;
                            Ok(Type::Optional(Box::new(member_type)))
                        }
                        _ => Ok(Type::Optional(Box::new(Type::Unknown))),
                    },
                    _ => Err(TypeError::simple(
                        format!("'?.' requires Optional<T>, found {obj_type}"),
                        object.span.clone(),
                    )),
                }
            }

            ExprKind::MemberAccess { object, member } => {
                let is_self = matches!(&object.kind, ExprKind::Identifier(n) if n == "self");
                let obj_type = self.infer_expr(object)?;
                self.resolve_member_access(&obj_type, member, &expr.span, is_self)
            }

            ExprKind::Index { object, index } => {
                let obj_type = self.infer_expr(object)?;
                let _idx_type = self.infer_expr(index)?;
                match &obj_type {
                    Type::Array(inner) => Ok(*inner.clone()),
                    Type::Dictionary(_, v) => Ok(*v.clone()),
                    _ => Ok(Type::Unknown),
                }
            }

            ExprKind::Cast {
                expr: inner,
                target_type,
            } => {
                let source = self.infer_expr(inner)?;
                let target = self.resolve_type_expr(target_type, &expr.span)?;
                if source == target || (source.is_numeric() && target.is_numeric()) {
                    Ok(target)
                } else {
                    Err(TypeError::simple(
                        format!("cannot cast {source} to {target}"),
                        expr.span.clone(),
                    ))
                }
            }

            ExprKind::Call { callee, args } => self.infer_call(callee, args, &expr.span),

            ExprKind::Lambda { params, body } => self.infer_lambda(params, body, &expr.span),

            ExprKind::Tuple(elements) => {
                let types: Vec<Type> = elements
                    .iter()
                    .map(|e| self.infer_expr(e))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Type::Tuple(types))
            }

            ExprKind::ErrorPropagate(inner) => self.infer_error_propagate(inner, &expr.span),

            ExprKind::ArrayLiteral(elements) => self.infer_array_literal(elements, &expr.span),

            ExprKind::DictLiteral(entries) => self.infer_dict_literal(entries, &expr.span),

            ExprKind::NamespaceAccess { namespace, member } => {
                self.infer_namespace_access(namespace, member, &expr.span)
            }

            ExprKind::When { subject, arms } => {
                if let Some(subj) = subject {
                    self.infer_expr(subj)?;
                }
                let mut result_type: Option<Type> = None;
                for arm in arms {
                    let arm_type = match &arm.body {
                        WhenBody::Expr(e) => self.infer_expr(e)?,
                        WhenBody::Block(stmts) => {
                            // Type is the type of the last expression statement, or Void
                            if let Some(last) = stmts.last() {
                                if let StmtKind::ExprStmt(e) = &last.kind {
                                    self.infer_expr(e)?
                                } else {
                                    Type::Void
                                }
                            } else {
                                Type::Void
                            }
                        }
                    };
                    if let Some(ref expected) = result_type {
                        if *expected != arm_type
                            && arm_type != Type::Unknown
                            && *expected != Type::Unknown
                        {
                            return Err(TypeError::simple(
                                format!(
                                    "when expression arms must return the same type: expected {expected}, found {arm_type}"
                                ),
                                expr.span.clone(),
                            ));
                        }
                    } else {
                        result_type = Some(arm_type);
                    }
                }
                Ok(result_type.unwrap_or(Type::Void))
            }

            ExprKind::Yield(arg) => {
                if let Some(inner) = arg {
                    self.infer_expr(inner)?;
                }
                // Yield expressions produce the value pushed by the scheduler on resume.
                // For now, treat as Unknown (full coroutine type checking is future work).
                Ok(Type::Unknown)
            }

            ExprKind::Super { method, args } => self.infer_super(method, args, &expr.span),
        }
    }

    pub(super) fn infer_binary(
        &mut self,
        op: &BinaryOp,
        lhs: &Expr,
        rhs: &Expr,
        span: &Span,
    ) -> Result<Type, TypeError> {
        let lhs_type = self.infer_expr(lhs)?;
        let rhs_type = self.infer_expr(rhs)?;

        match op {
            BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::Modulo => {
                if !lhs_type.is_numeric() {
                    return Err(TypeError::simple(
                        format!(
                            "left operand of arithmetic operator must be numeric, found {lhs_type}"
                        ),
                        lhs.span.clone(),
                    ));
                }
                if !rhs_type.is_numeric() {
                    return Err(TypeError::simple(
                        format!(
                            "right operand of arithmetic operator must be numeric, found {rhs_type}"
                        ),
                        rhs.span.clone(),
                    ));
                }
                if lhs_type != rhs_type {
                    return Err(TypeError::simple(
                        format!(
                            "arithmetic operands must be the same type: {lhs_type} vs {rhs_type}"
                        ),
                        span.clone(),
                    ));
                }
                Ok(lhs_type)
            }

            BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::Greater
            | BinaryOp::LessEqual
            | BinaryOp::GreaterEqual => {
                if lhs_type != rhs_type {
                    return Err(TypeError::simple(
                        format!(
                            "comparison operands must be the same type: {lhs_type} vs {rhs_type}"
                        ),
                        span.clone(),
                    ));
                }
                Ok(Type::Bool)
            }

            BinaryOp::And | BinaryOp::Or => {
                if lhs_type != Type::Bool {
                    return Err(TypeError::simple(
                        format!("left operand of logical operator must be bool, found {lhs_type}"),
                        lhs.span.clone(),
                    ));
                }
                if rhs_type != Type::Bool {
                    return Err(TypeError::simple(
                        format!("right operand of logical operator must be bool, found {rhs_type}"),
                        rhs.span.clone(),
                    ));
                }
                Ok(Type::Bool)
            }
        }
    }

    pub(super) fn infer_unary(&mut self, op: &UnaryOp, operand: &Expr) -> Result<Type, TypeError> {
        let operand_type = self.infer_expr(operand)?;
        match op {
            UnaryOp::Negate => {
                if !operand_type.is_numeric() {
                    return Err(TypeError::simple(
                        format!("cannot negate non-numeric type {operand_type}"),
                        operand.span.clone(),
                    ));
                }
                Ok(operand_type)
            }
            UnaryOp::Not => {
                if operand_type != Type::Bool {
                    return Err(TypeError::simple(
                        format!("cannot apply '!' to non-bool type {operand_type}"),
                        operand.span.clone(),
                    ));
                }
                Ok(Type::Bool)
            }
        }
    }

    pub(super) fn infer_ternary(
        &mut self,
        condition: &Expr,
        then_expr: &Expr,
        else_expr: &Expr,
        span: &Span,
    ) -> Result<Type, TypeError> {
        let cond_type = self.infer_expr(condition)?;
        if cond_type != Type::Bool {
            return Err(TypeError::simple(
                format!("ternary condition must be bool, found {cond_type}"),
                condition.span.clone(),
            ));
        }

        let then_type = self.infer_expr(then_expr)?;
        let else_type = self.infer_expr(else_expr)?;
        if then_type != else_type {
            return Err(TypeError::simple(
                format!("ternary branches must have the same type: {then_type} vs {else_type}"),
                span.clone(),
            ));
        }
        Ok(then_type)
    }

    // --- Phase 6: Functions + Return Types ---

    pub(super) fn infer_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: &Span,
    ) -> Result<Type, TypeError> {
        // Special-case Success() and Error() constructors for Result<T>.
        if let ExprKind::Identifier(name) = &callee.kind {
            match name.as_str() {
                "Success" => {
                    if args.len() != 1 {
                        return Err(TypeError::simple(
                            format!("Success() expects 1 argument, found {}", args.len()),
                            span.clone(),
                        ));
                    }
                    let arg_expr = call_arg_expr(&args[0]);
                    let inner_type = self.infer_expr(arg_expr)?;
                    return Ok(Type::Result(Box::new(inner_type)));
                }
                "Error" => {
                    if args.len() != 1 {
                        return Err(TypeError::simple(
                            format!("Error() expects 1 argument, found {}", args.len()),
                            span.clone(),
                        ));
                    }
                    let arg_expr = call_arg_expr(&args[0]);
                    let arg_type = self.infer_expr(arg_expr)?;
                    if arg_type != Type::Str {
                        return Err(TypeError::simple(
                            format!("Error() expects a string message, found {arg_type}"),
                            arg_expr.span.clone(),
                        ));
                    }
                    return Ok(Type::Result(Box::new(Type::Unknown)));
                }
                _ => {}
            }
        }

        // Check if this is a constructor call for a registered class.
        if let ExprKind::Identifier(name) = &callee.kind
            && let Some(class_info) = self.registry.get_class(name).cloned()
        {
            return self.infer_constructor_call(&class_info, args, span);
        }

        // Check if this is a constructor call for a registered struct.
        if let ExprKind::Identifier(name) = &callee.kind
            && let Some(struct_info) = self.registry.get_struct(name).cloned()
        {
            return self.infer_struct_constructor_call(&struct_info, args, span);
        }

        let callee_type = self.infer_expr(callee)?;
        match callee_type {
            Type::Function {
                params,
                return_type,
            } => {
                // Untyped sentinel: a single Unknown param signals "accept any args".
                // Used by register_host_fn_untyped -- still infers arg exprs for
                // undefined-variable detection but skips arity and type checks.
                if params.len() == 1 && params[0] == Type::Unknown {
                    for arg in args {
                        self.infer_expr(call_arg_expr(arg))?;
                    }
                    return Ok(*return_type);
                }
                if args.len() != params.len() {
                    return Err(TypeError::simple(
                        format!(
                            "expected {} argument(s), found {}",
                            params.len(),
                            args.len()
                        ),
                        span.clone(),
                    ));
                }
                for (i, (arg, expected_type)) in args.iter().zip(params.iter()).enumerate() {
                    let arg_expr = call_arg_expr(arg);
                    let actual = self.infer_expr(arg_expr)?;
                    if !self.types_compatible(expected_type, &actual) {
                        return Err(TypeError::simple(
                            format!(
                                "argument {} type mismatch: expected {expected_type}, found {actual}",
                                i + 1
                            ),
                            arg_expr.span.clone(),
                        ));
                    }
                }
                Ok(*return_type)
            }
            _ => Err(TypeError::simple(
                format!("type '{callee_type}' is not callable"),
                callee.span.clone(),
            )),
        }
    }

    pub(super) fn infer_super(
        &mut self,
        method: &str,
        args: &[CallArg],
        span: &Span,
    ) -> Result<Type, TypeError> {
        // Resolve the enclosing class and its parent.
        let class_name = self.current_class.clone().ok_or_else(|| {
            TypeError::simple(
                "'super' used outside of a class method".to_string(),
                span.clone(),
            )
        })?;
        let parent_name = self
            .registry
            .get_class(&class_name)
            .and_then(|c| c.parent.clone())
            .ok_or_else(|| {
                TypeError::simple(
                    format!("'super' used in '{class_name}' which has no parent class"),
                    span.clone(),
                )
            })?;

        // Find the method on the parent class (searches the full parent chain).
        let method_info = self
            .registry
            .all_methods(&parent_name)
            .into_iter()
            .find(|m| m.name == method)
            .ok_or_else(|| {
                TypeError::simple(
                    format!("parent class '{parent_name}' has no method '{method}'"),
                    span.clone(),
                )
            })?;

        // Type-check arguments against the parent method's parameter types.
        // Skip the implicit `self` parameter (first param in the method info is self's type,
        // but MethodInfo.params only stores explicit params -- consistent with infer_call).
        let params = &method_info.params;
        if args.len() != params.len() {
            return Err(TypeError::simple(
                format!(
                    "super.{method}() expects {} argument(s), found {}",
                    params.len(),
                    args.len()
                ),
                span.clone(),
            ));
        }
        for (i, (arg, expected)) in args.iter().zip(params.iter()).enumerate() {
            let arg_expr = call_arg_expr(arg);
            let actual = self.infer_expr(arg_expr)?;
            if !self.types_compatible(expected, &actual) {
                return Err(TypeError::simple(
                    format!(
                        "super.{method}() argument {} type mismatch: expected {expected}, found {actual}",
                        i + 1
                    ),
                    arg_expr.span.clone(),
                ));
            }
        }

        Ok(method_info.return_type.clone())
    }

    pub(super) fn infer_error_propagate(
        &mut self,
        inner: &Expr,
        span: &Span,
    ) -> Result<Type, TypeError> {
        let inner_type = self.infer_expr(inner)?;
        let unwrapped = match &inner_type {
            Type::Result(t) => t.as_ref().clone(),
            _ => {
                return Err(TypeError::simple(
                    format!("'?' operator requires Result<T>, found {inner_type}"),
                    inner.span.clone(),
                ));
            }
        };
        match &self.current_return_type {
            Some(Type::Result(_)) => Ok(unwrapped),
            _ => Err(TypeError::with_suggestions(
                "'?' operator can only be used inside a function returning Result<T>".to_string(),
                span.clone(),
                vec![suggestions::Suggestion {
                    message: "the enclosing function must return Result<T> to use '?'".to_string(),
                    replacement: None,
                    span: span.clone(),
                }],
            )),
        }
    }

    pub(super) fn infer_null_coalesce(
        &mut self,
        lhs: &Expr,
        rhs: &Expr,
        span: &Span,
    ) -> Result<Type, TypeError> {
        let lhs_type = self.infer_expr(lhs)?;
        let rhs_type = self.infer_expr(rhs)?;

        match &lhs_type {
            Type::Optional(inner) => {
                if **inner != Type::Unknown && **inner != rhs_type {
                    return Err(TypeError::simple(
                        format!(
                            "null coalescing type mismatch: Optional<{inner}> cannot fallback to {rhs_type}"
                        ),
                        rhs.span.clone(),
                    ));
                }
                Ok(rhs_type)
            }
            Type::Result(inner) => {
                if **inner != Type::Unknown && **inner != rhs_type {
                    return Err(TypeError::simple(
                        format!(
                            "null coalescing type mismatch: Result<{inner}> cannot fallback to {rhs_type}"
                        ),
                        rhs.span.clone(),
                    ));
                }
                Ok(rhs_type)
            }
            _ => Err(TypeError::simple(
                format!("'??' requires Optional<T> or Result<T> on the left, found {lhs_type}"),
                span.clone(),
            )),
        }
    }

    pub(super) fn infer_lambda(
        &mut self,
        params: &[crate::parser::FuncParam],
        body: &LambdaBody,
        span: &Span,
    ) -> Result<Type, TypeError> {
        let mut param_types = Vec::new();
        self.env.push_scope();

        for param in params {
            let ty = self.resolve_type_expr(&param.type_annotation, span)?;
            param_types.push(ty.clone());
            self.env.define(
                &param.name,
                VarInfo {
                    ty,
                    mutability: Mutability::Immutable,
                },
            );
        }

        let return_type = match body {
            LambdaBody::Expr(e) => self.infer_expr(e)?,
            LambdaBody::Block(stmts) => {
                for stmt in stmts {
                    self.check_stmt(stmt)?;
                }
                Type::Void
            }
        };

        self.env.pop_scope();

        Ok(Type::Function {
            params: param_types,
            return_type: Box::new(return_type),
        })
    }
}
