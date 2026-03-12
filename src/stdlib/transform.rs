use std::cell::RefCell;
use std::rc::Rc;

use crate::vm::{VM, Value, WritObject};
use glam::{Mat3, Mat4, Quat, Vec2, Vec3};

use super::matrix::{mat3_value, mat4_value};
use super::quaternion::{extract_quat, quat_value};
use super::vector::{
    extract_f32, extract_vec2, extract_vec3, value_to_vec2, value_to_vec3, vec2_value, vec3_value,
};

// --- Transform2D ---

#[derive(Debug, Clone)]
pub struct WritTransform2D {
    pub position: Vec2,
    pub rotation: f32,
    pub scale: Vec2,
}

impl WritObject for WritTransform2D {
    fn type_name(&self) -> &str {
        "Transform2D"
    }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        match name {
            "position" => Ok(vec2_value(self.position)),
            "rotation" => Ok(Value::F64(self.rotation as f64)),
            "scale" => Ok(vec2_value(self.scale)),
            _ => Err(format!("Transform2D has no field '{name}'")),
        }
    }

    fn set_field(&mut self, name: &str, value: Value) -> Result<(), String> {
        match name {
            "position" => {
                self.position = value_to_vec2(&value)?;
                Ok(())
            }
            "rotation" => {
                self.rotation = extract_f32(&value, "rotation")?;
                Ok(())
            }
            "scale" => {
                self.scale = value_to_vec2(&value)?;
                Ok(())
            }
            _ => Err(format!("Transform2D has no field '{name}'")),
        }
    }

    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "toMatrix" => {
                let m =
                    Mat3::from_scale_angle_translation(self.scale, self.rotation, self.position);
                Ok(mat3_value(m))
            }
            "inverse" => Ok(transform2d_value(WritTransform2D {
                position: -self.position,
                rotation: -self.rotation,
                scale: Vec2::new(1.0 / self.scale.x, 1.0 / self.scale.y),
            })),
            "transformPoint" => {
                let v = extract_vec2(args, 0)?;
                let rotated = Vec2::new(
                    v.x * self.rotation.cos() - v.y * self.rotation.sin(),
                    v.x * self.rotation.sin() + v.y * self.rotation.cos(),
                );
                Ok(vec2_value(rotated * self.scale + self.position))
            }
            "transformVector" => {
                let v = extract_vec2(args, 0)?;
                let rotated = Vec2::new(
                    v.x * self.rotation.cos() - v.y * self.rotation.sin(),
                    v.x * self.rotation.sin() + v.y * self.rotation.cos(),
                );
                Ok(vec2_value(rotated * self.scale))
            }
            "translate" => {
                let offset = extract_vec2(args, 0)?;
                self.position += offset;
                Ok(Value::Null)
            }
            "rotate" => {
                let angle = extract_f32(args.first().ok_or("missing angle argument")?, "angle")?;
                self.rotation += angle;
                Ok(Value::Null)
            }
            "lookAt" => {
                let target = extract_vec2(args, 0)?;
                let diff = target - self.position;
                self.rotation = diff.y.atan2(diff.x);
                Ok(Value::Null)
            }
            _ => Err(format!("Transform2D has no method '{name}'")),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn clone_box(&self) -> Box<dyn WritObject> {
        Box::new(self.clone())
    }
}

// --- Transform3D ---

#[derive(Debug, Clone)]
pub struct WritTransform3D {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl WritObject for WritTransform3D {
    fn type_name(&self) -> &str {
        "Transform3D"
    }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        match name {
            "position" => Ok(vec3_value(self.position)),
            "rotation" => Ok(quat_value(self.rotation)),
            "scale" => Ok(vec3_value(self.scale)),
            _ => Err(format!("Transform3D has no field '{name}'")),
        }
    }

    fn set_field(&mut self, name: &str, value: Value) -> Result<(), String> {
        match name {
            "position" => {
                self.position = value_to_vec3(&value)?;
                Ok(())
            }
            "rotation" => {
                self.rotation = extract_quat(&[value], 0)?;
                Ok(())
            }
            "scale" => {
                self.scale = value_to_vec3(&value)?;
                Ok(())
            }
            _ => Err(format!("Transform3D has no field '{name}'")),
        }
    }

    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "toMatrix" => {
                let m =
                    Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position);
                Ok(mat4_value(m))
            }
            "inverse" => Ok(transform3d_value(WritTransform3D {
                position: -(self.rotation.inverse() * self.position),
                rotation: self.rotation.inverse(),
                scale: Vec3::new(1.0 / self.scale.x, 1.0 / self.scale.y, 1.0 / self.scale.z),
            })),
            "transformPoint" => {
                let v = extract_vec3(args, 0)?;
                Ok(vec3_value(self.rotation * (v * self.scale) + self.position))
            }
            "transformVector" => {
                let v = extract_vec3(args, 0)?;
                Ok(vec3_value(self.rotation * (v * self.scale)))
            }
            "translate" => {
                let offset = extract_vec3(args, 0)?;
                self.position += offset;
                Ok(Value::Null)
            }
            "rotate" => {
                let axis = extract_vec3(args, 0)?;
                let angle = extract_f32(args.get(1).ok_or("missing angle argument")?, "angle")?;
                self.rotation = Quat::from_axis_angle(axis, angle) * self.rotation;
                Ok(Value::Null)
            }
            "lookAt" => {
                let target = extract_vec3(args, 0)?;
                let up = extract_vec3(args, 1)?;
                let forward = (target - self.position).normalize();
                let right = up.cross(forward).normalize();
                let corrected_up = forward.cross(right);
                self.rotation =
                    Quat::from_mat3(&glam::Mat3::from_cols(right, corrected_up, forward));
                Ok(Value::Null)
            }
            _ => Err(format!("Transform3D has no method '{name}'")),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn clone_box(&self) -> Box<dyn WritObject> {
        Box::new(self.clone())
    }
}

// --- Value constructors ---

fn transform2d_value(t: WritTransform2D) -> Value {
    Value::Object(Rc::new(RefCell::new(t)))
}

fn transform3d_value(t: WritTransform3D) -> Value {
    Value::Object(Rc::new(RefCell::new(t)))
}

// --- Registration ---

pub fn register(vm: &mut VM) {
    vm.register_type("Transform2D", |args| {
        if args.is_empty() {
            return Ok(Box::new(WritTransform2D {
                position: Vec2::ZERO,
                rotation: 0.0,
                scale: Vec2::ONE,
            }));
        }
        let pos = value_to_vec2(args.first().ok_or("missing position")?)?;
        let rot = extract_f32(args.get(1).ok_or("missing rotation")?, "rotation")?;
        let scale = value_to_vec2(args.get(2).ok_or("missing scale")?)?;
        Ok(Box::new(WritTransform2D {
            position: pos,
            rotation: rot,
            scale,
        }))
    });

    vm.register_type("Transform3D", |args| {
        if args.is_empty() {
            return Ok(Box::new(WritTransform3D {
                position: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            }));
        }
        let pos = value_to_vec3(args.first().ok_or("missing position")?)?;
        let rot = extract_quat(args, 1)?;
        let scale = value_to_vec3(args.get(2).ok_or("missing scale")?)?;
        Ok(Box::new(WritTransform3D {
            position: pos,
            rotation: rot,
            scale,
        }))
    });
}
