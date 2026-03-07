use std::cell::RefCell;
use std::rc::Rc;

use glam::{Vec2, Vec3, Vec4};
use crate::vm::WritObject;
use crate::vm::{VM, Value};

// ── Vector2 ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WritVector2(pub Vec2);

impl WritObject for WritVector2 {
    fn type_name(&self) -> &str {
        "Vector2"
    }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        match name {
            "x" => Ok(Value::F64(self.0.x as f64)),
            "y" => Ok(Value::F64(self.0.y as f64)),
            _ => Err(format!("Vector2 has no field '{name}'")),
        }
    }

    fn set_field(&mut self, name: &str, value: Value) -> Result<(), String> {
        let f = extract_f32(&value, name)?;
        match name {
            "x" => self.0.x = f,
            "y" => self.0.y = f,
            _ => return Err(format!("Vector2 has no field '{name}'")),
        }
        Ok(())
    }

    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "length" => Ok(Value::F64(self.0.length() as f64)),
            "lengthSquared" => Ok(Value::F64(self.0.length_squared() as f64)),
            "normalized" => Ok(vec2_value(self.0.normalize_or_zero())),
            "dot" => {
                let other = extract_vec2(args, 0)?;
                Ok(Value::F64(self.0.dot(other) as f64))
            }
            "distance" => {
                let other = extract_vec2(args, 0)?;
                Ok(Value::F64(self.0.distance(other) as f64))
            }
            "distanceSquared" => {
                let other = extract_vec2(args, 0)?;
                Ok(Value::F64(self.0.distance_squared(other) as f64))
            }
            "lerp" => {
                let other = extract_vec2(args, 0)?;
                let t = extract_f32_arg(args, 1, "t")?;
                Ok(vec2_value(self.0.lerp(other, t)))
            }
            "clamp" => {
                let min = extract_vec2(args, 0)?;
                let max = extract_vec2(args, 1)?;
                Ok(vec2_value(self.0.clamp(min, max)))
            }
            "abs" => Ok(vec2_value(self.0.abs())),
            "sign" => Ok(vec2_value(self.0.signum())),
            "floor" => Ok(vec2_value(self.0.floor())),
            "ceil" => Ok(vec2_value(self.0.ceil())),
            "round" => Ok(vec2_value(self.0.round())),
            "min" => {
                let other = extract_vec2(args, 0)?;
                Ok(vec2_value(self.0.min(other)))
            }
            "max" => {
                let other = extract_vec2(args, 0)?;
                Ok(vec2_value(self.0.max(other)))
            }
            "add" => {
                let other = extract_vec2(args, 0)?;
                Ok(vec2_value(self.0 + other))
            }
            "sub" => {
                let other = extract_vec2(args, 0)?;
                Ok(vec2_value(self.0 - other))
            }
            "mul" => {
                if let Some(other) = try_extract_vec2(args, 0) {
                    Ok(vec2_value(self.0 * other))
                } else {
                    let s = extract_f32_arg(args, 0, "scalar")?;
                    Ok(vec2_value(self.0 * s))
                }
            }
            "div" => {
                if let Some(other) = try_extract_vec2(args, 0) {
                    Ok(vec2_value(self.0 / other))
                } else {
                    let s = extract_f32_arg(args, 0, "scalar")?;
                    Ok(vec2_value(self.0 / s))
                }
            }
            "negate" => Ok(vec2_value(-self.0)),
            _ => Err(format!("Vector2 has no method '{name}'")),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ── Vector3 ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WritVector3(pub Vec3);

impl WritObject for WritVector3 {
    fn type_name(&self) -> &str {
        "Vector3"
    }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        match name {
            "x" => Ok(Value::F64(self.0.x as f64)),
            "y" => Ok(Value::F64(self.0.y as f64)),
            "z" => Ok(Value::F64(self.0.z as f64)),
            _ => Err(format!("Vector3 has no field '{name}'")),
        }
    }

    fn set_field(&mut self, name: &str, value: Value) -> Result<(), String> {
        let f = extract_f32(&value, name)?;
        match name {
            "x" => self.0.x = f,
            "y" => self.0.y = f,
            "z" => self.0.z = f,
            _ => return Err(format!("Vector3 has no field '{name}'")),
        }
        Ok(())
    }

    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "length" => Ok(Value::F64(self.0.length() as f64)),
            "lengthSquared" => Ok(Value::F64(self.0.length_squared() as f64)),
            "normalized" => Ok(vec3_value(self.0.normalize_or_zero())),
            "dot" => {
                let other = extract_vec3(args, 0)?;
                Ok(Value::F64(self.0.dot(other) as f64))
            }
            "cross" => {
                let other = extract_vec3(args, 0)?;
                Ok(vec3_value(self.0.cross(other)))
            }
            "distance" => {
                let other = extract_vec3(args, 0)?;
                Ok(Value::F64(self.0.distance(other) as f64))
            }
            "distanceSquared" => {
                let other = extract_vec3(args, 0)?;
                Ok(Value::F64(self.0.distance_squared(other) as f64))
            }
            "lerp" => {
                let other = extract_vec3(args, 0)?;
                let t = extract_f32_arg(args, 1, "t")?;
                Ok(vec3_value(self.0.lerp(other, t)))
            }
            "clamp" => {
                let min = extract_vec3(args, 0)?;
                let max = extract_vec3(args, 1)?;
                Ok(vec3_value(self.0.clamp(min, max)))
            }
            "abs" => Ok(vec3_value(self.0.abs())),
            "sign" => Ok(vec3_value(self.0.signum())),
            "floor" => Ok(vec3_value(self.0.floor())),
            "ceil" => Ok(vec3_value(self.0.ceil())),
            "round" => Ok(vec3_value(self.0.round())),
            "min" => {
                let other = extract_vec3(args, 0)?;
                Ok(vec3_value(self.0.min(other)))
            }
            "max" => {
                let other = extract_vec3(args, 0)?;
                Ok(vec3_value(self.0.max(other)))
            }
            "add" => {
                let other = extract_vec3(args, 0)?;
                Ok(vec3_value(self.0 + other))
            }
            "sub" => {
                let other = extract_vec3(args, 0)?;
                Ok(vec3_value(self.0 - other))
            }
            "mul" => {
                if let Some(other) = try_extract_vec3(args, 0) {
                    Ok(vec3_value(self.0 * other))
                } else {
                    let s = extract_f32_arg(args, 0, "scalar")?;
                    Ok(vec3_value(self.0 * s))
                }
            }
            "div" => {
                if let Some(other) = try_extract_vec3(args, 0) {
                    Ok(vec3_value(self.0 / other))
                } else {
                    let s = extract_f32_arg(args, 0, "scalar")?;
                    Ok(vec3_value(self.0 / s))
                }
            }
            "negate" => Ok(vec3_value(-self.0)),
            _ => Err(format!("Vector3 has no method '{name}'")),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ── Vector4 ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WritVector4(pub Vec4);

impl WritObject for WritVector4 {
    fn type_name(&self) -> &str {
        "Vector4"
    }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        match name {
            "x" => Ok(Value::F64(self.0.x as f64)),
            "y" => Ok(Value::F64(self.0.y as f64)),
            "z" => Ok(Value::F64(self.0.z as f64)),
            "w" => Ok(Value::F64(self.0.w as f64)),
            _ => Err(format!("Vector4 has no field '{name}'")),
        }
    }

    fn set_field(&mut self, name: &str, value: Value) -> Result<(), String> {
        let f = extract_f32(&value, name)?;
        match name {
            "x" => self.0.x = f,
            "y" => self.0.y = f,
            "z" => self.0.z = f,
            "w" => self.0.w = f,
            _ => return Err(format!("Vector4 has no field '{name}'")),
        }
        Ok(())
    }

    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "length" => Ok(Value::F64(self.0.length() as f64)),
            "lengthSquared" => Ok(Value::F64(self.0.length_squared() as f64)),
            "normalized" => Ok(vec4_value(self.0.normalize_or_zero())),
            "dot" => {
                let other = extract_vec4(args, 0)?;
                Ok(Value::F64(self.0.dot(other) as f64))
            }
            "distance" => {
                let other = extract_vec4(args, 0)?;
                Ok(Value::F64(self.0.distance(other) as f64))
            }
            "distanceSquared" => {
                let other = extract_vec4(args, 0)?;
                Ok(Value::F64(self.0.distance_squared(other) as f64))
            }
            "lerp" => {
                let other = extract_vec4(args, 0)?;
                let t = extract_f32_arg(args, 1, "t")?;
                Ok(vec4_value(self.0.lerp(other, t)))
            }
            "clamp" => {
                let min = extract_vec4(args, 0)?;
                let max = extract_vec4(args, 1)?;
                Ok(vec4_value(self.0.clamp(min, max)))
            }
            "abs" => Ok(vec4_value(self.0.abs())),
            "sign" => Ok(vec4_value(self.0.signum())),
            "floor" => Ok(vec4_value(self.0.floor())),
            "ceil" => Ok(vec4_value(self.0.ceil())),
            "round" => Ok(vec4_value(self.0.round())),
            "min" => {
                let other = extract_vec4(args, 0)?;
                Ok(vec4_value(self.0.min(other)))
            }
            "max" => {
                let other = extract_vec4(args, 0)?;
                Ok(vec4_value(self.0.max(other)))
            }
            "add" => {
                let other = extract_vec4(args, 0)?;
                Ok(vec4_value(self.0 + other))
            }
            "sub" => {
                let other = extract_vec4(args, 0)?;
                Ok(vec4_value(self.0 - other))
            }
            "mul" => {
                if let Some(other) = try_extract_vec4(args, 0) {
                    Ok(vec4_value(self.0 * other))
                } else {
                    let s = extract_f32_arg(args, 0, "scalar")?;
                    Ok(vec4_value(self.0 * s))
                }
            }
            "div" => {
                if let Some(other) = try_extract_vec4(args, 0) {
                    Ok(vec4_value(self.0 / other))
                } else {
                    let s = extract_f32_arg(args, 0, "scalar")?;
                    Ok(vec4_value(self.0 / s))
                }
            }
            "negate" => Ok(vec4_value(-self.0)),
            _ => Err(format!("Vector4 has no method '{name}'")),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ── Value constructors ──────────────────────────────────────────────

pub fn vec2_value(v: Vec2) -> Value {
    Value::Object(Rc::new(RefCell::new(WritVector2(v))))
}

pub fn vec3_value(v: Vec3) -> Value {
    Value::Object(Rc::new(RefCell::new(WritVector3(v))))
}

pub fn vec4_value(v: Vec4) -> Value {
    Value::Object(Rc::new(RefCell::new(WritVector4(v))))
}

// ── Extraction helpers ──────────────────────────────────────────────

pub fn extract_f32(value: &Value, context: &str) -> Result<f32, String> {
    match value {
        Value::F32(f) => Ok(*f),
        Value::F64(f) => Ok(*f as f32),
        Value::I32(n) => Ok(*n as f32),
        Value::I64(n) => Ok(*n as f32),
        other => Err(format!(
            "{context}: expected number, got {}",
            other.type_name()
        )),
    }
}

pub fn extract_f64(value: &Value, context: &str) -> Result<f64, String> {
    match value {
        Value::F32(f) => Ok(*f as f64),
        Value::F64(f) => Ok(*f),
        Value::I32(n) => Ok(*n as f64),
        Value::I64(n) => Ok(*n as f64),
        other => Err(format!(
            "{context}: expected number, got {}",
            other.type_name()
        )),
    }
}

fn extract_f32_arg(args: &[Value], idx: usize, name: &str) -> Result<f32, String> {
    let v = args
        .get(idx)
        .ok_or_else(|| format!("missing argument '{name}'"))?;
    extract_f32(v, name)
}

fn field_f32(obj: &dyn WritObject, name: &str) -> Result<f32, String> {
    extract_f32(&obj.get_field(name)?, name)
}

pub fn extract_vec2(args: &[Value], idx: usize) -> Result<Vec2, String> {
    let v = args.get(idx).ok_or("missing Vector2 argument")?;
    value_to_vec2(v)
}

pub fn extract_vec3(args: &[Value], idx: usize) -> Result<Vec3, String> {
    let v = args.get(idx).ok_or("missing Vector3 argument")?;
    value_to_vec3(v)
}

pub fn extract_vec4(args: &[Value], idx: usize) -> Result<Vec4, String> {
    let v = args.get(idx).ok_or("missing Vector4 argument")?;
    value_to_vec4(v)
}

pub fn value_to_vec2(v: &Value) -> Result<Vec2, String> {
    match v {
        Value::Object(obj) => {
            let b = obj.borrow();
            if b.type_name() != "Vector2" {
                return Err(format!("expected Vector2, got {}", b.type_name()));
            }
            Ok(Vec2::new(field_f32(&*b, "x")?, field_f32(&*b, "y")?))
        }
        other => Err(format!("expected Vector2, got {}", other.type_name())),
    }
}

pub fn value_to_vec3(v: &Value) -> Result<Vec3, String> {
    match v {
        Value::Object(obj) => {
            let b = obj.borrow();
            if b.type_name() != "Vector3" {
                return Err(format!("expected Vector3, got {}", b.type_name()));
            }
            Ok(Vec3::new(
                field_f32(&*b, "x")?,
                field_f32(&*b, "y")?,
                field_f32(&*b, "z")?,
            ))
        }
        other => Err(format!("expected Vector3, got {}", other.type_name())),
    }
}

pub fn value_to_vec4(v: &Value) -> Result<Vec4, String> {
    match v {
        Value::Object(obj) => {
            let b = obj.borrow();
            if b.type_name() != "Vector4" {
                return Err(format!("expected Vector4, got {}", b.type_name()));
            }
            Ok(Vec4::new(
                field_f32(&*b, "x")?,
                field_f32(&*b, "y")?,
                field_f32(&*b, "z")?,
                field_f32(&*b, "w")?,
            ))
        }
        other => Err(format!("expected Vector4, got {}", other.type_name())),
    }
}

fn try_extract_vec2(args: &[Value], idx: usize) -> Option<Vec2> {
    extract_vec2(args, idx).ok()
}

fn try_extract_vec3(args: &[Value], idx: usize) -> Option<Vec3> {
    extract_vec3(args, idx).ok()
}

fn try_extract_vec4(args: &[Value], idx: usize) -> Option<Vec4> {
    extract_vec4(args, idx).ok()
}

// ── Registration ────────────────────────────────────────────────────

pub fn register(vm: &mut VM) {
    // Vector2 constructor
    vm.register_type("Vector2", |args| {
        let x = extract_f32(args.first().ok_or("Vector2: missing x")?, "x")?;
        let y = extract_f32(args.get(1).ok_or("Vector2: missing y")?, "y")?;
        Ok(Box::new(WritVector2(Vec2::new(x, y))))
    });

    // Vector2 constants
    vm.register_global("Vector2_ZERO", vec2_value(Vec2::ZERO));
    vm.register_global("Vector2_ONE", vec2_value(Vec2::ONE));
    vm.register_global("Vector2_UP", vec2_value(Vec2::new(0.0, -1.0)));
    vm.register_global("Vector2_DOWN", vec2_value(Vec2::new(0.0, 1.0)));
    vm.register_global("Vector2_LEFT", vec2_value(Vec2::new(-1.0, 0.0)));
    vm.register_global("Vector2_RIGHT", vec2_value(Vec2::new(1.0, 0.0)));

    // Vector3 constructor
    vm.register_type("Vector3", |args| {
        let x = extract_f32(args.first().ok_or("Vector3: missing x")?, "x")?;
        let y = extract_f32(args.get(1).ok_or("Vector3: missing y")?, "y")?;
        let z = extract_f32(args.get(2).ok_or("Vector3: missing z")?, "z")?;
        Ok(Box::new(WritVector3(Vec3::new(x, y, z))))
    });

    // Vector3 constants
    vm.register_global("Vector3_ZERO", vec3_value(Vec3::ZERO));
    vm.register_global("Vector3_ONE", vec3_value(Vec3::ONE));
    vm.register_global("Vector3_UP", vec3_value(Vec3::new(0.0, 1.0, 0.0)));
    vm.register_global("Vector3_DOWN", vec3_value(Vec3::new(0.0, -1.0, 0.0)));
    vm.register_global("Vector3_FORWARD", vec3_value(Vec3::new(0.0, 0.0, -1.0)));
    vm.register_global("Vector3_BACK", vec3_value(Vec3::new(0.0, 0.0, 1.0)));

    // Vector4 constructor
    vm.register_type("Vector4", |args| {
        let x = extract_f32(args.first().ok_or("Vector4: missing x")?, "x")?;
        let y = extract_f32(args.get(1).ok_or("Vector4: missing y")?, "y")?;
        let z = extract_f32(args.get(2).ok_or("Vector4: missing z")?, "z")?;
        let w = extract_f32(args.get(3).ok_or("Vector4: missing w")?, "w")?;
        Ok(Box::new(WritVector4(Vec4::new(x, y, z, w))))
    });
}
