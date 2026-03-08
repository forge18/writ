use std::collections::HashMap;

use crate::parser::Visibility;

use super::types::Type;

/// Metadata for a class field.
#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: String,
    pub ty: Type,
    pub visibility: Visibility,
    pub has_default: bool,
    pub has_setter: bool,
}

/// Metadata for a method (class method, trait method, or enum method).
#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub name: String,
    /// Parameter types (does NOT include `self`).
    pub params: Vec<Type>,
    pub return_type: Type,
    pub is_static: bool,
    pub visibility: Visibility,
    /// Whether the method has a default body (for trait methods).
    pub has_default_body: bool,
}

/// Stored metadata for a class declaration.
#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub name: String,
    pub fields: Vec<FieldInfo>,
    pub methods: Vec<MethodInfo>,
    pub parent: Option<String>,
    pub traits: Vec<String>,
}

/// Stored metadata for a trait declaration.
#[derive(Debug, Clone)]
pub struct TraitInfo {
    pub name: String,
    pub methods: Vec<MethodInfo>,
}

/// Stored metadata for an enum declaration.
#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub name: String,
    pub variants: Vec<String>,
    pub fields: Vec<FieldInfo>,
    pub methods: Vec<MethodInfo>,
}

/// Stored metadata for a struct declaration.
/// Value type -- no inheritance, no traits.
#[derive(Debug, Clone)]
pub struct StructInfo {
    pub name: String,
    pub fields: Vec<FieldInfo>,
    pub methods: Vec<MethodInfo>,
}

/// Registry of user-defined types (classes, traits, enums, structs).
///
/// Type definitions are registered during the first pass over declarations,
/// then queried during type checking for member resolution, constructor
/// synthesis, trait validation, and subtype checking.
pub struct TypeRegistry {
    classes: HashMap<String, ClassInfo>,
    traits: HashMap<String, TraitInfo>,
    enums: HashMap<String, EnumInfo>,
    structs: HashMap<String, StructInfo>,
}

impl Default for TypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeRegistry {
    pub fn new() -> Self {
        Self {
            classes: HashMap::new(),
            traits: HashMap::new(),
            enums: HashMap::new(),
            structs: HashMap::new(),
        }
    }

    pub fn register_class(&mut self, info: ClassInfo) {
        self.classes.insert(info.name.clone(), info);
    }

    pub fn register_trait(&mut self, info: TraitInfo) {
        self.traits.insert(info.name.clone(), info);
    }

    pub fn register_enum(&mut self, info: EnumInfo) {
        self.enums.insert(info.name.clone(), info);
    }

    pub fn get_class(&self, name: &str) -> Option<&ClassInfo> {
        self.classes.get(name)
    }

    pub fn get_trait(&self, name: &str) -> Option<&TraitInfo> {
        self.traits.get(name)
    }

    pub fn get_enum(&self, name: &str) -> Option<&EnumInfo> {
        self.enums.get(name)
    }

    pub fn register_struct(&mut self, info: StructInfo) {
        self.structs.insert(info.name.clone(), info);
    }

    pub fn get_struct(&self, name: &str) -> Option<&StructInfo> {
        self.structs.get(name)
    }

    /// Returns `true` if any class, trait, enum, or struct is registered with this name.
    /// Returns an iterator over all registered class names.
    pub fn class_names(&self) -> impl Iterator<Item = &str> {
        self.classes.keys().map(|s| s.as_str())
    }

    /// Returns an iterator over all registered trait names.
    pub fn trait_names(&self) -> impl Iterator<Item = &str> {
        self.traits.keys().map(|s| s.as_str())
    }

    /// Returns an iterator over all registered enum names.
    pub fn enum_names(&self) -> impl Iterator<Item = &str> {
        self.enums.keys().map(|s| s.as_str())
    }

    /// Returns an iterator over all registered struct names.
    pub fn struct_names(&self) -> impl Iterator<Item = &str> {
        self.structs.keys().map(|s| s.as_str())
    }

    pub fn has_type(&self, name: &str) -> bool {
        self.classes.contains_key(name)
            || self.traits.contains_key(name)
            || self.enums.contains_key(name)
            || self.structs.contains_key(name)
    }

    /// Returns `true` if `child` is a subclass of `ancestor` (walking the
    /// inheritance chain). Returns `false` if `child == ancestor`.
    pub fn is_subclass(&self, child: &str, ancestor: &str) -> bool {
        let mut current = child.to_string();
        loop {
            let info = match self.classes.get(&current) {
                Some(info) => info,
                None => return false,
            };
            match &info.parent {
                Some(parent) => {
                    if parent == ancestor {
                        return true;
                    }
                    current = parent.clone();
                }
                None => return false,
            }
        }
    }

    /// Returns all fields for a class, including inherited fields from the
    /// parent chain. Parent fields appear first; child fields can shadow them.
    pub fn all_fields(&self, class_name: &str) -> Vec<FieldInfo> {
        let mut chain = Vec::new();
        let mut current = class_name.to_string();
        while let Some(info) = self.classes.get(&current) {
            chain.push(info);
            match &info.parent {
                Some(parent) => current = parent.clone(),
                None => break,
            }
        }

        // Reverse so parent fields come first.
        let mut fields = Vec::new();
        for info in chain.iter().rev() {
            for field in &info.fields {
                fields.push(field.clone());
            }
        }
        fields
    }

    /// Returns all methods for a class, including inherited methods and trait
    /// default methods. Later entries override earlier ones (child > parent > trait).
    pub fn all_methods(&self, class_name: &str) -> Vec<MethodInfo> {
        let mut chain = Vec::new();
        let mut current = class_name.to_string();
        while let Some(info) = self.classes.get(&current) {
            chain.push(info);
            match &info.parent {
                Some(parent) => current = parent.clone(),
                None => break,
            }
        }

        let mut methods: Vec<MethodInfo> = Vec::new();
        let mut seen: HashMap<String, usize> = HashMap::new();

        // Walk from root parent down to child.
        for info in chain.iter().rev() {
            // Trait default methods (lower priority than own methods).
            for trait_name in &info.traits {
                if let Some(trait_info) = self.traits.get(trait_name) {
                    for method in &trait_info.methods {
                        if method.has_default_body {
                            if let Some(&idx) = seen.get(&method.name) {
                                methods[idx] = method.clone();
                            } else {
                                seen.insert(method.name.clone(), methods.len());
                                methods.push(method.clone());
                            }
                        }
                    }
                }
            }

            // Own methods (highest priority at this level).
            for method in &info.methods {
                if let Some(&idx) = seen.get(&method.name) {
                    methods[idx] = method.clone();
                } else {
                    seen.insert(method.name.clone(), methods.len());
                    methods.push(method.clone());
                }
            }
        }

        methods
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_class(name: &str, parent: Option<&str>) -> ClassInfo {
        ClassInfo {
            name: name.to_string(),
            fields: Vec::new(),
            methods: Vec::new(),
            parent: parent.map(|s| s.to_string()),
            traits: Vec::new(),
        }
    }

    #[test]
    fn register_and_get_class() {
        let mut reg = TypeRegistry::new();
        reg.register_class(ClassInfo {
            name: "Player".to_string(),
            fields: vec![FieldInfo {
                name: "health".to_string(),
                ty: Type::Float,
                visibility: Visibility::Public,
                has_default: true,
                has_setter: false,
            }],
            methods: Vec::new(),
            parent: None,
            traits: Vec::new(),
        });
        let info = reg.get_class("Player").unwrap();
        assert_eq!(info.name, "Player");
        assert_eq!(info.fields.len(), 1);
        assert_eq!(info.fields[0].name, "health");
    }

    #[test]
    fn register_and_get_trait() {
        let mut reg = TypeRegistry::new();
        reg.register_trait(TraitInfo {
            name: "Damageable".to_string(),
            methods: vec![MethodInfo {
                name: "takeDamage".to_string(),
                params: vec![Type::Float],
                return_type: Type::Void,
                is_static: false,
                visibility: Visibility::Public,
                has_default_body: false,
            }],
        });
        let info = reg.get_trait("Damageable").unwrap();
        assert_eq!(info.methods.len(), 1);
        assert_eq!(info.methods[0].name, "takeDamage");
    }

    #[test]
    fn register_and_get_enum() {
        let mut reg = TypeRegistry::new();
        reg.register_enum(EnumInfo {
            name: "Direction".to_string(),
            variants: vec![
                "North".to_string(),
                "South".to_string(),
                "East".to_string(),
                "West".to_string(),
            ],
            fields: Vec::new(),
            methods: Vec::new(),
        });
        let info = reg.get_enum("Direction").unwrap();
        assert_eq!(info.variants.len(), 4);
    }

    #[test]
    fn has_type_checks_all_maps() {
        let mut reg = TypeRegistry::new();
        assert!(!reg.has_type("Player"));
        reg.register_class(sample_class("Player", None));
        assert!(reg.has_type("Player"));

        reg.register_trait(TraitInfo {
            name: "Runnable".to_string(),
            methods: Vec::new(),
        });
        assert!(reg.has_type("Runnable"));

        reg.register_enum(EnumInfo {
            name: "Color".to_string(),
            variants: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
        });
        assert!(reg.has_type("Color"));

        assert!(!reg.has_type("Missing"));
    }

    #[test]
    fn is_subclass_direct() {
        let mut reg = TypeRegistry::new();
        reg.register_class(sample_class("Entity", None));
        reg.register_class(sample_class("Player", Some("Entity")));
        assert!(reg.is_subclass("Player", "Entity"));
    }

    #[test]
    fn is_subclass_transitive() {
        let mut reg = TypeRegistry::new();
        reg.register_class(sample_class("Base", None));
        reg.register_class(sample_class("Middle", Some("Base")));
        reg.register_class(sample_class("Child", Some("Middle")));
        assert!(reg.is_subclass("Child", "Base"));
        assert!(reg.is_subclass("Child", "Middle"));
    }

    #[test]
    fn is_subclass_false() {
        let mut reg = TypeRegistry::new();
        reg.register_class(sample_class("A", None));
        reg.register_class(sample_class("B", None));
        assert!(!reg.is_subclass("A", "B"));
        assert!(!reg.is_subclass("A", "A"));
    }

    #[test]
    fn all_fields_with_inheritance() {
        let mut reg = TypeRegistry::new();
        reg.register_class(ClassInfo {
            name: "Entity".to_string(),
            fields: vec![FieldInfo {
                name: "id".to_string(),
                ty: Type::Int,
                visibility: Visibility::Public,
                has_default: false,
                has_setter: false,
            }],
            methods: Vec::new(),
            parent: None,
            traits: Vec::new(),
        });
        reg.register_class(ClassInfo {
            name: "Player".to_string(),
            fields: vec![FieldInfo {
                name: "name".to_string(),
                ty: Type::Str,
                visibility: Visibility::Public,
                has_default: false,
                has_setter: false,
            }],
            methods: Vec::new(),
            parent: Some("Entity".to_string()),
            traits: Vec::new(),
        });

        let fields = reg.all_fields("Player");
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "id"); // parent first
        assert_eq!(fields[1].name, "name");
    }

    #[test]
    fn all_methods_includes_trait_defaults_and_overrides() {
        let mut reg = TypeRegistry::new();
        reg.register_trait(TraitInfo {
            name: "Greetable".to_string(),
            methods: vec![MethodInfo {
                name: "greet".to_string(),
                params: Vec::new(),
                return_type: Type::Str,
                is_static: false,
                visibility: Visibility::Public,
                has_default_body: true,
            }],
        });
        reg.register_class(ClassInfo {
            name: "Player".to_string(),
            fields: Vec::new(),
            methods: vec![MethodInfo {
                name: "attack".to_string(),
                params: Vec::new(),
                return_type: Type::Void,
                is_static: false,
                visibility: Visibility::Public,
                has_default_body: false,
            }],
            parent: None,
            traits: vec!["Greetable".to_string()],
        });

        let methods = reg.all_methods("Player");
        assert_eq!(methods.len(), 2);
        let names: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"greet"));
        assert!(names.contains(&"attack"));
    }
}
