use std::cell::RefCell;
use std::rc::Rc;

use crate::vm::{VM, Value, WritObject};
use glam::{Mat3, Mat4, Vec3};

use super::vector::{extract_f32, vec2_value, vec3_value};

// ── Matrix3 ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WritMatrix3(pub Mat3);

impl WritObject for WritMatrix3 {
    fn type_name(&self) -> &str {
        "Matrix3"
    }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        Err(format!("Matrix3 has no field '{name}'"))
    }

    fn set_field(&mut self, name: &str, _value: Value) -> Result<(), String> {
        Err(format!("Matrix3 has no field '{name}'"))
    }

    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "determinant" => Ok(Value::F64(self.0.determinant() as f64)),
            "inverse" => Ok(mat3_value(self.0.inverse())),
            "transpose" => Ok(mat3_value(self.0.transpose())),
            "multiply" => {
                let other = extract_mat3(args, 0)?;
                Ok(mat3_value(self.0 * other))
            }
            "transformPoint" => {
                let v = super::vector::extract_vec2(args, 0)?;
                let result = self.0.mul_vec3(glam::Vec3::new(v.x, v.y, 1.0));
                Ok(vec2_value(glam::Vec2::new(result.x, result.y)))
            }
            "transformVector" => {
                let v = super::vector::extract_vec2(args, 0)?;
                let result = self.0.mul_vec3(glam::Vec3::new(v.x, v.y, 0.0));
                Ok(vec2_value(glam::Vec2::new(result.x, result.y)))
            }
            _ => Err(format!("Matrix3 has no method '{name}'")),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ── Matrix4 ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WritMatrix4(pub Mat4);

impl WritObject for WritMatrix4 {
    fn type_name(&self) -> &str {
        "Matrix4"
    }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        Err(format!("Matrix4 has no field '{name}'"))
    }

    fn set_field(&mut self, name: &str, _value: Value) -> Result<(), String> {
        Err(format!("Matrix4 has no field '{name}'"))
    }

    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "determinant" => Ok(Value::F64(self.0.determinant() as f64)),
            "inverse" => Ok(mat4_value(self.0.inverse())),
            "transpose" => Ok(mat4_value(self.0.transpose())),
            "multiply" => {
                let other = extract_mat4(args, 0)?;
                Ok(mat4_value(self.0 * other))
            }
            "transformPoint" => {
                let v = super::vector::extract_vec3(args, 0)?;
                Ok(vec3_value(self.0.transform_point3(v)))
            }
            "transformVector" => {
                let v = super::vector::extract_vec3(args, 0)?;
                Ok(vec3_value(self.0.transform_vector3(v)))
            }
            _ => Err(format!("Matrix4 has no method '{name}'")),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ── Value constructors ──────────────────────────────────────────────

pub fn mat3_value(m: Mat3) -> Value {
    Value::Object(Rc::new(RefCell::new(WritMatrix3(m))))
}

pub fn mat4_value(m: Mat4) -> Value {
    Value::Object(Rc::new(RefCell::new(WritMatrix4(m))))
}

// ── Extraction helpers ──────────────────────────────────────────────

pub fn extract_mat3(args: &[Value], idx: usize) -> Result<Mat3, String> {
    let v = args.get(idx).ok_or("missing Matrix3 argument")?;
    match v {
        Value::Object(obj) => {
            let b = obj.borrow();
            b.as_any()
                .downcast_ref::<WritMatrix3>()
                .map(|m| m.0)
                .ok_or_else(|| format!("expected Matrix3, got {}", b.type_name()))
        }
        other => Err(format!("expected Matrix3, got {}", other.type_name())),
    }
}

pub fn extract_mat4(args: &[Value], idx: usize) -> Result<Mat4, String> {
    let v = args.get(idx).ok_or("missing Matrix4 argument")?;
    match v {
        Value::Object(obj) => {
            let b = obj.borrow();
            b.as_any()
                .downcast_ref::<WritMatrix4>()
                .map(|m| m.0)
                .ok_or_else(|| format!("expected Matrix4, got {}", b.type_name()))
        }
        other => Err(format!("expected Matrix4, got {}", other.type_name())),
    }
}

// ── Registration ────────────────────────────────────────────────────

pub fn register(vm: &mut VM) {
    // Matrix3 constants
    vm.register_global("Matrix3_IDENTITY", mat3_value(Mat3::IDENTITY));
    vm.register_global("Matrix3_ZERO", mat3_value(Mat3::ZERO));

    // Matrix3 factories
    vm.register_fn_in_module(
        "Matrix3_rotation",
        "matrix",
        crate::vm::binding::fn1(|angle: f64| -> Result<Value, String> {
            Ok(mat3_value(Mat3::from_angle(angle as f32)))
        }),
    );
    vm.register_fn_in_module(
        "Matrix3_scale",
        "matrix",
        crate::vm::binding::fn2(|sx: f64, sy: f64| -> Result<Value, String> {
            Ok(mat3_value(Mat3::from_scale(glam::Vec2::new(
                sx as f32, sy as f32,
            ))))
        }),
    );
    vm.register_fn_in_module(
        "Matrix3_translation",
        "matrix",
        crate::vm::binding::fn2(|tx: f64, ty: f64| -> Result<Value, String> {
            Ok(mat3_value(Mat3::from_translation(glam::Vec2::new(
                tx as f32, ty as f32,
            ))))
        }),
    );

    // Matrix4 constants
    vm.register_global("Matrix4_IDENTITY", mat4_value(Mat4::IDENTITY));
    vm.register_global("Matrix4_ZERO", mat4_value(Mat4::ZERO));

    // Matrix4 factories
    vm.register_fn_in_module(
        "Matrix4_rotation",
        "matrix",
        crate::vm::binding::fn2(|axis: Value, angle: f64| -> Result<Value, String> {
            let ax = super::vector::value_to_vec3(&axis)?;
            Ok(mat4_value(Mat4::from_axis_angle(ax, angle as f32)))
        }),
    );
    vm.register_fn_in_module(
        "Matrix4_scale",
        "matrix",
        crate::vm::binding::fn3(|sx: f64, sy: f64, sz: f64| -> Result<Value, String> {
            Ok(mat4_value(Mat4::from_scale(Vec3::new(
                sx as f32, sy as f32, sz as f32,
            ))))
        }),
    );
    vm.register_fn_in_module(
        "Matrix4_translation",
        "matrix",
        crate::vm::binding::fn3(|tx: f64, ty: f64, tz: f64| -> Result<Value, String> {
            Ok(mat4_value(Mat4::from_translation(Vec3::new(
                tx as f32, ty as f32, tz as f32,
            ))))
        }),
    );

    // Matrix4.perspective — 4 args, use raw NativeFn
    let perspective_fn: RawNativeFn = Rc::new(|args: &[Value]| {
        let fov = extract_f32(args.first().ok_or("missing fov")?, "fov")?;
        let aspect = extract_f32(args.get(1).ok_or("missing aspect")?, "aspect")?;
        let near = extract_f32(args.get(2).ok_or("missing near")?, "near")?;
        let far = extract_f32(args.get(3).ok_or("missing far")?, "far")?;
        Ok(mat4_value(Mat4::perspective_rh(fov, aspect, near, far)))
    });
    vm.register_fn_in_module("Matrix4_perspective", "matrix", RawFn(perspective_fn));

    // Matrix4.orthographic — 6 args
    let ortho_fn: RawNativeFn = Rc::new(|args: &[Value]| {
        let left = extract_f32(args.first().ok_or("missing left")?, "left")?;
        let right = extract_f32(args.get(1).ok_or("missing right")?, "right")?;
        let bottom = extract_f32(args.get(2).ok_or("missing bottom")?, "bottom")?;
        let top = extract_f32(args.get(3).ok_or("missing top")?, "top")?;
        let near = extract_f32(args.get(4).ok_or("missing near")?, "near")?;
        let far = extract_f32(args.get(5).ok_or("missing far")?, "far")?;
        Ok(mat4_value(Mat4::orthographic_rh(
            left, right, bottom, top, near, far,
        )))
    });
    vm.register_fn_in_module("Matrix4_orthographic", "matrix", RawFn(ortho_fn));

    // Matrix4.lookAt
    vm.register_fn_in_module(
        "Matrix4_lookAt",
        "matrix",
        crate::vm::binding::fn3(
            |eye: Value, target: Value, up: Value| -> Result<Value, String> {
                let e = super::vector::value_to_vec3(&eye)?;
                let t = super::vector::value_to_vec3(&target)?;
                let u = super::vector::value_to_vec3(&up)?;
                Ok(mat4_value(Mat4::look_at_rh(e, t, u)))
            },
        ),
    );
}

/// Type alias for raw native function closures.
pub(crate) type RawNativeFn = Rc<dyn Fn(&[Value]) -> Result<Value, String>>;

/// Raw function handler for functions with >3 args.
pub(crate) struct RawFn(pub RawNativeFn);

impl crate::vm::IntoNativeHandler for RawFn {
    fn arity() -> Option<u8> {
        None
    }
    fn into_handler(self) -> crate::vm::NativeFn {
        self.0
    }
}
