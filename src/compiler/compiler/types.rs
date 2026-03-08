use super::*;

impl Compiler {
    pub(super) fn compile_struct_decl(&mut self, decl: &StructDecl, span: &Span) -> Result<(), CompileError> {
        // Generic templates are not compiled — only their monomorphic instantiations are.
        if !decl.type_params.is_empty() {
            return Ok(());
        }

        let line = span.line;
        let struct_name = &decl.name;

        let field_names: Vec<String> = decl.fields.iter().map(|f| f.name.clone()).collect();
        let public_fields: HashSet<String> = decl
            .fields
            .iter()
            .filter(|f| f.visibility == Visibility::Public)
            .map(|f| f.name.clone())
            .collect();

        // Compile methods
        let mut public_methods = HashSet::new();
        for method in &decl.methods {
            let qualified_name = format!("{}::{}", struct_name, method.name);
            public_methods.insert(method.name.clone());
            self.compile_method_body(&qualified_name, &method.params, &method.body, span, true)?;
        }

        // Compile constructor
        {
            let saved = self.save_state();
            self.scope_depth = 0;
            self.has_yield = false;
            self.next_reg = 0;
            self.max_reg = 0;

            for field in &decl.fields {
                self.add_local(&field.name, span)?;
            }

            let field_count = u8::try_from(decl.fields.len()).map_err(|_| CompileError {
                annotation: None,
                message: "too many struct fields (max 255)".to_string(),
                span: span.clone(),
            })?;
            let name_idx = self.chunk.add_string(struct_name);
            let result_reg = self.alloc_temp(span)?;
            self.emit(
                Instruction::MakeStruct(result_reg, name_idx, 0, field_count),
                line,
            );
            self.emit(Instruction::Return(result_reg), line);

            let ctor_chunk = std::mem::take(&mut self.chunk);
            let ctor_max = self.max_reg;

            self.restore_state(saved);

            let arity = u8::try_from(decl.fields.len()).map_err(|_| CompileError {
                annotation: None,
                message: "too many struct fields for constructor (max 255)".to_string(),
                span: span.clone(),
            })?;

            let func_idx = self.functions.len();
            self.function_index
                .insert(struct_name.clone(), func_idx as u16);
            self.functions.push(CompiledFunction {
                name: struct_name.clone(),
                arity,
                chunk: ctor_chunk,
                is_coroutine: false,
                is_variadic: false,
                upvalues: Vec::new(),
                max_registers: ctor_max,
                has_rc_values: true, // creates Struct value
            });
        }

        self.struct_metas.push(StructMeta {
            name: struct_name.clone(),
            field_names,
            public_fields,
            public_methods,
        });

        Ok(())
    }

    // ── Class compilation ──────────────────────────────────────

    pub(super) fn compile_class_decl(&mut self, decl: &ClassDecl, span: &Span) -> Result<(), CompileError> {
        // Generic templates are not compiled — only their monomorphic instantiations are.
        if !decl.type_params.is_empty() {
            return Ok(());
        }

        let line = span.line;
        let class_name = &decl.name;

        // Use pre-registered field names when available (supports forward declarations).
        // Pre-registration via pre_register_classes() resolves the full inheritance chain.
        let (all_field_names, own_field_names) =
            if let Some(pre) = self.class_metas.iter().find(|m| m.name == *class_name) {
                (pre.field_names.clone(), pre.own_field_names.clone())
            } else {
                let parent_field_names: Vec<String> = if let Some(parent) = &decl.extends {
                    self.class_metas
                        .iter()
                        .find(|m| m.name == *parent)
                        .map(|m| m.field_names.clone())
                        .unwrap_or_default()
                } else {
                    vec![]
                };
                let own: Vec<String> = decl.fields.iter().map(|f| f.name.clone()).collect();
                let mut all = parent_field_names;
                all.extend(own.clone());
                (all, own)
            };

        let public_fields: HashSet<String> = decl
            .fields
            .iter()
            .filter(|f| f.visibility == Visibility::Public)
            .map(|f| f.name.clone())
            .collect();

        // Compile methods (track current class for super dispatch)
        let prev_class = self.current_class.take();
        self.current_class = Some(class_name.clone());
        let mut public_methods = HashSet::new();
        for method in &decl.methods {
            let qualified_name = format!("{}::{}", class_name, method.name);
            public_methods.insert(method.name.clone());
            self.compile_method_body(&qualified_name, &method.params, &method.body, span, true)?;
        }
        self.current_class = prev_class;

        // Compile constructor
        {
            let saved = self.save_state();
            self.scope_depth = 0;
            self.has_yield = false;
            self.next_reg = 0;
            self.max_reg = 0;

            for field_name in &all_field_names {
                self.add_local(field_name, span)?;
            }

            let field_count = u8::try_from(all_field_names.len()).map_err(|_| CompileError {
                annotation: None,
                message: "too many class fields (max 255)".to_string(),
                span: span.clone(),
            })?;
            let name_idx = self.chunk.add_string(class_name);
            let result_reg = self.alloc_temp(span)?;
            self.emit(
                Instruction::MakeClass(result_reg, name_idx, 0, field_count),
                line,
            );
            self.emit(Instruction::Return(result_reg), line);

            let ctor_chunk = std::mem::take(&mut self.chunk);
            let ctor_max = self.max_reg;

            self.restore_state(saved);

            let arity = u8::try_from(all_field_names.len()).map_err(|_| CompileError {
                annotation: None,
                message: "too many class fields for constructor (max 255)".to_string(),
                span: span.clone(),
            })?;

            let func_idx = self.functions.len();
            self.function_index
                .insert(class_name.clone(), func_idx as u16);
            self.functions.push(CompiledFunction {
                name: class_name.clone(),
                arity,
                chunk: ctor_chunk,
                is_coroutine: false,
                is_variadic: false,
                upvalues: Vec::new(),
                max_registers: ctor_max,
                has_rc_values: true, // creates Object value
            });
        }

        // Update or insert the class meta (update if pre-registered, insert otherwise).
        let meta = ClassMeta {
            name: class_name.clone(),
            field_names: all_field_names,
            own_field_names,
            public_fields,
            public_methods,
            parent: decl.extends.clone(),
        };
        if let Some(existing) = self.class_metas.iter_mut().find(|m| m.name == *class_name) {
            *existing = meta;
        } else {
            self.class_metas.push(meta);
        }

        Ok(())
    }

    /// Pre-registers all class shapes from a statement list before full compilation.
    ///
    /// This allows child classes to appear before their parent in source order.
    /// Pass 1: register each class with only its own fields.
    /// Pass 2: resolve inheritance by prepending parent field lists.
    pub fn pre_register_classes(&mut self, stmts: &[Stmt]) {
        // Pass 1: register all classes with their own fields only.
        for stmt in stmts {
            let decl = match &stmt.kind {
                StmtKind::Class(d) => d,
                StmtKind::Export(inner) => match &inner.kind {
                    StmtKind::Class(d) => d,
                    _ => continue,
                },
                _ => continue,
            };
            if !decl.type_params.is_empty() {
                continue;
            }
            // Skip if already registered (e.g. from a previous load call).
            if self.class_metas.iter().any(|m| m.name == decl.name) {
                continue;
            }
            let own_field_names: Vec<String> = decl.fields.iter().map(|f| f.name.clone()).collect();
            let public_fields: HashSet<String> = decl
                .fields
                .iter()
                .filter(|f| f.visibility == Visibility::Public)
                .map(|f| f.name.clone())
                .collect();
            let public_methods: HashSet<String> =
                decl.methods.iter().map(|m| m.name.clone()).collect();
            self.class_metas.push(ClassMeta {
                name: decl.name.clone(),
                field_names: own_field_names.clone(), // temporary — resolved in pass 2
                own_field_names,
                public_fields,
                public_methods,
                parent: decl.extends.clone(),
            });
        }

        // Pass 2: resolve field_names to include inherited fields (parent fields first).
        // Iterate until stable (handles multi-level inheritance).
        let names: Vec<String> = self.class_metas.iter().map(|m| m.name.clone()).collect();
        for name in &names {
            let resolved = self.resolve_inherited_fields(name);
            if let Some(meta) = self.class_metas.iter_mut().find(|m| m.name == *name) {
                meta.field_names = resolved;
            }
        }
    }

    /// Resolves the complete field list for a class by walking the inheritance chain.
    pub(super) fn resolve_inherited_fields(&self, class_name: &str) -> Vec<String> {
        let Some(meta) = self.class_metas.iter().find(|m| m.name == class_name) else {
            return Vec::new();
        };
        let own = meta.own_field_names.clone();
        let parent = meta.parent.clone();
        match parent {
            None => own,
            Some(parent_name) => {
                let mut fields = self.resolve_inherited_fields(&parent_name);
                fields.extend(own);
                fields
            }
        }
    }

    /// Helper to compile a method body (struct or class method).
    pub(super) fn compile_method_body(
        &mut self,
        qualified_name: &str,
        params: &[crate::parser::FuncParam],
        body: &[Stmt],
        span: &Span,
        has_self: bool,
    ) -> Result<(), CompileError> {
        let line = span.line;

        let saved = self.save_state();
        self.scope_depth = 0;
        self.has_yield = false;
        self.next_reg = 0;
        self.max_reg = 0;

        if has_self {
            self.add_local("self", span)?;
        }
        for param in params {
            self.add_local(&param.name, span)?;
        }

        for stmt in body {
            self.compile_stmt(stmt)?;
        }

        if self.chunk.is_empty()
            || !matches!(
                self.chunk.last_opcode(),
                Some(op::Return) | Some(op::ReturnNull)
            )
        {
            self.emit(Instruction::ReturnNull, line);
        }

        let method_chunk = std::mem::take(&mut self.chunk);
        let is_coroutine = self.has_yield;
        let method_max = self.max_reg;

        self.restore_state(saved);

        let arity = u8::try_from(params.len() + if has_self { 1 } else { 0 }).map_err(|_| {
            CompileError {
                annotation: None,
                message: "too many method parameters (max 254)".to_string(),
                span: span.clone(),
            }
        })?;

        let method_idx = self.functions.len() as u16;
        self.function_index.insert(qualified_name.to_string(), method_idx);
        self.functions.push(CompiledFunction {
            name: qualified_name.to_string(),
            arity,
            chunk: method_chunk,
            is_coroutine,
            is_variadic: false,
            upvalues: Vec::new(),
            max_registers: method_max,
            has_rc_values: true, // methods receive self (Object/Struct)
        });

        Ok(())
    }

    /// Compile `super.method(args)` — directly calls the parent class's method
    /// implementation, bypassing the VM's runtime method lookup.
    pub(super) fn compile_super(
        &mut self,
        method: &str,
        args: &[CallArg],
        span: &Span,
        dst: Option<u8>,
    ) -> Result<u8, CompileError> {
        let line = span.line;

        // Find the parent class name from the current class meta.
        let parent_name = self
            .current_class
            .as_ref()
            .and_then(|cls| self.class_metas.iter().find(|m| m.name == *cls))
            .and_then(|m| m.parent.clone())
            .ok_or_else(|| CompileError {
                annotation: None,
                message: "'super' used outside of a class method with a parent".to_string(),
                span: span.clone(),
            })?;

        // Build the qualified method name and look it up in the function index.
        let qualified = format!("{parent_name}::{method}");
        let func_idx = self.function_index.get(&qualified).copied().ok_or_else(|| CompileError {
            annotation: None,
            message: format!("parent class '{parent_name}' has no compiled method '{method}'"),
            span: span.clone(),
        })?;

        // Emit: [self, arg0, arg1, ...] in consecutive registers, then CallDirect.
        // `self` is always register 0 in a method body.
        let base = self.next_reg;
        let self_reg = self.alloc_temp(span)?;
        // Load `self` (local 0) into the call's base register.
        self.emit(Instruction::Move(self_reg, 0), line);

        for arg in args {
            let arg_reg = self.alloc_temp(span)?;
            self.compile_call_arg(arg, Some(arg_reg))?;
        }

        let arity = u8::try_from(args.len() + 1).map_err(|_| CompileError {
            annotation: None,
            message: "too many arguments for super call (max 254)".to_string(),
            span: span.clone(),
        })?;

        self.emit(Instruction::CallDirect(base, func_idx, arity), line);

        // Result is in `base`; free arg temps.
        self.next_reg = base + 1;
        self.set_reg_type(base, ExprType::Other);

        if let Some(d) = dst {
            if d != base {
                self.emit(Instruction::Move(d, base), line);
                self.next_reg = base;
            }
            return Ok(d);
        }
        Ok(base)
    }

}
