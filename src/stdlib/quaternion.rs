use std::cell::RefCell;
use std::rc::Rc;

use crate::vm::{VM, Value, WritObject};
use glam::Quat;

use super::matrix::mat4_value;
use super::vector::{extract_f32, extract_vec3, value_to_vec3, vec3_value};

#[derive(Debug, Clone)]
pub struct WritQuaternion(pub Quat);

impl WritObject for WritQuaternion {
    fn type_name(&self) -> &str {
        "Quaternion"
    }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        match name {
            "x" => Ok(Value::F64(self.0.x as f64)),
            "y" => Ok(Value::F64(self.0.y as f64)),
            "z" => Ok(Value::F64(self.0.z as f64)),
            "w" => Ok(Value::F64(self.0.w as f64)),
            _ => Err(format!("Quaternion has no field '{name}'")),
        }
    }

    fn set_field(&mut self, name: &str, value: Value) -> Result<(), String> {
        let f = extract_f32(&value, name)?;
        match name {
            "x" => self.0.x = f,
            "y" => self.0.y = f,
            "z" => self.0.z = f,
            "w" => self.0.w = f,
            _ => return Err(format!("Quaternion has no field '{name}'")),
        }
        Ok(())
    }

    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "normalized" => Ok(quat_value(self.0.normalize())),
            "inverse" => Ok(quat_value(self.0.inverse())),
            "dot" => {
                let other = extract_quat(args, 0)?;
                Ok(Value::F64(self.0.dot(other) as f64))
            }
            "slerp" => {
                let other = extract_quat(args, 0)?;
                let t = extract_f32(args.get(1).ok_or("missing argument 't'")?, "t")?;
                Ok(quat_value(self.0.slerp(other, t)))
            }
            "lerp" => {
                let other = extract_quat(args, 0)?;
                let t = extract_f32(args.get(1).ok_or("missing argument 't'")?, "t")?;
                Ok(quat_value(self.0.lerp(other, t)))
            }
            "toEuler" => {
                let (x, y, z) = self.0.to_euler(glam::EulerRot::XYZ);
                Ok(vec3_value(glam::Vec3::new(x, y, z)))
            }
            "toMatrix" => Ok(mat4_value(glam::Mat4::from_quat(self.0))),
            "rotate" => {
                let v = extract_vec3(args, 0)?;
                Ok(vec3_value(self.0 * v))
            }
            "mul" => {
                let other = extract_quat(args, 0)?;
                Ok(quat_value(self.0 * other))
            }
            _ => Err(format!("Quaternion has no method '{name}'")),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn clone_box(&self) -> Box<dyn WritObject> {
        Box::new(self.clone())
    }
}

pub fn quat_value(q: Quat) -> Value {
    Value::Object(Rc::new(RefCell::new(WritQuaternion(q))))
}

pub fn extract_quat(args: &[Value], idx: usize) -> Result<Quat, String> {
    let v = args.get(idx).ok_or("missing Quaternion argument")?;
    match v {
        Value::Object(obj) => {
            let b = obj.borrow();
            b.as_any()
                .downcast_ref::<WritQuaternion>()
                .map(|q| q.0)
                .ok_or_else(|| format!("expected Quaternion, got {}", b.type_name()))
        }
        other => Err(format!("expected Quaternion, got {}", other.type_name())),
    }
}

pub fn register(vm: &mut VM) {
    vm.register_global("Quaternion_IDENTITY", quat_value(Quat::IDENTITY));

    vm.register_fn_in_module(
        "Quaternion_fromAxisAngle",
        "quaternion",
        crate::vm::binding::fn2(|axis: Value, angle: f64| -> Result<Value, String> {
            let ax = value_to_vec3(&axis)?;
            Ok(quat_value(Quat::from_axis_angle(ax, angle as f32)))
        }),
    );

    vm.register_fn_in_module(
        "Quaternion_fromEuler",
        "quaternion",
        crate::vm::binding::fn3(|x: f64, y: f64, z: f64| -> Result<Value, String> {
            Ok(quat_value(Quat::from_euler(
                glam::EulerRot::XYZ,
                x as f32,
                y as f32,
                z as f32,
            )))
        }),
    );

    vm.register_fn_in_module(
        "Quaternion_lookRotation",
        "quaternion",
        crate::vm::binding::fn2(|forward: Value, up: Value| -> Result<Value, String> {
            let f = value_to_vec3(&forward)?;
            let u = value_to_vec3(&up)?;
            let mat = glam::Mat4::look_to_rh(glam::Vec3::ZERO, f, u);
            Ok(quat_value(Quat::from_mat4(&mat)))
        }),
    );
}
