use std::cell::RefCell;
use std::rc::Rc;

use glam::{Vec2, Vec3};
use crate::vm::{VM, Value, WritObject};

use super::vector::{extract_f32, extract_vec2, extract_vec3, vec2_value, vec3_value};

// ── Rectangle ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WritRectangle {
    pub position: Vec2,
    pub size: Vec2,
}

impl WritObject for WritRectangle {
    fn type_name(&self) -> &str {
        "Rectangle"
    }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        match name {
            "position" => Ok(vec2_value(self.position)),
            "size" => Ok(vec2_value(self.size)),
            _ => Err(format!("Rectangle has no field '{name}'")),
        }
    }

    fn set_field(&mut self, name: &str, value: Value) -> Result<(), String> {
        match name {
            "position" => {
                self.position = super::vector::value_to_vec2(&value)?;
                Ok(())
            }
            "size" => {
                self.size = super::vector::value_to_vec2(&value)?;
                Ok(())
            }
            _ => Err(format!("Rectangle has no field '{name}'")),
        }
    }

    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "width" => Ok(Value::F64(self.size.x as f64)),
            "height" => Ok(Value::F64(self.size.y as f64)),
            "center" => Ok(vec2_value(self.position + self.size * 0.5)),
            "area" => Ok(Value::F64((self.size.x * self.size.y) as f64)),
            "contains" => {
                let p = extract_vec2(args, 0)?;
                let inside = p.x >= self.position.x
                    && p.y >= self.position.y
                    && p.x <= self.position.x + self.size.x
                    && p.y <= self.position.y + self.size.y;
                Ok(Value::Bool(inside))
            }
            "intersects" => {
                let other = extract_rect(args, 0)?;
                let intersects = self.position.x < other.position.x + other.size.x
                    && self.position.x + self.size.x > other.position.x
                    && self.position.y < other.position.y + other.size.y
                    && self.position.y + self.size.y > other.position.y;
                Ok(Value::Bool(intersects))
            }
            "intersection" => {
                let other = extract_rect(args, 0)?;
                let x1 = self.position.x.max(other.position.x);
                let y1 = self.position.y.max(other.position.y);
                let x2 = (self.position.x + self.size.x).min(other.position.x + other.size.x);
                let y2 = (self.position.y + self.size.y).min(other.position.y + other.size.y);
                if x2 > x1 && y2 > y1 {
                    Ok(rect_value(WritRectangle {
                        position: Vec2::new(x1, y1),
                        size: Vec2::new(x2 - x1, y2 - y1),
                    }))
                } else {
                    Ok(Value::Null)
                }
            }
            "merge" => {
                let other = extract_rect(args, 0)?;
                let min = self.position.min(other.position);
                let max = (self.position + self.size).max(other.position + other.size);
                Ok(rect_value(WritRectangle {
                    position: min,
                    size: max - min,
                }))
            }
            "expand" => {
                let amount = extract_f32(args.first().ok_or("missing amount argument")?, "amount")?;
                Ok(rect_value(WritRectangle {
                    position: self.position - Vec2::splat(amount),
                    size: self.size + Vec2::splat(amount * 2.0),
                }))
            }
            _ => Err(format!("Rectangle has no method '{name}'")),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ── BoundingBox ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WritBoundingBox {
    pub min: Vec3,
    pub max: Vec3,
}

impl WritObject for WritBoundingBox {
    fn type_name(&self) -> &str {
        "BoundingBox"
    }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        match name {
            "min" => Ok(vec3_value(self.min)),
            "max" => Ok(vec3_value(self.max)),
            _ => Err(format!("BoundingBox has no field '{name}'")),
        }
    }

    fn set_field(&mut self, name: &str, value: Value) -> Result<(), String> {
        match name {
            "min" => {
                self.min = super::vector::value_to_vec3(&value)?;
                Ok(())
            }
            "max" => {
                self.max = super::vector::value_to_vec3(&value)?;
                Ok(())
            }
            _ => Err(format!("BoundingBox has no field '{name}'")),
        }
    }

    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "size" => Ok(vec3_value(self.max - self.min)),
            "center" => Ok(vec3_value((self.min + self.max) * 0.5)),
            "volume" => {
                let s = self.max - self.min;
                Ok(Value::F64((s.x * s.y * s.z) as f64))
            }
            "contains" => {
                let p = extract_vec3(args, 0)?;
                let inside = p.x >= self.min.x
                    && p.y >= self.min.y
                    && p.z >= self.min.z
                    && p.x <= self.max.x
                    && p.y <= self.max.y
                    && p.z <= self.max.z;
                Ok(Value::Bool(inside))
            }
            "intersects" => {
                let other = extract_bbox(args, 0)?;
                let intersects = self.min.x <= other.max.x
                    && self.max.x >= other.min.x
                    && self.min.y <= other.max.y
                    && self.max.y >= other.min.y
                    && self.min.z <= other.max.z
                    && self.max.z >= other.min.z;
                Ok(Value::Bool(intersects))
            }
            "intersection" => {
                let other = extract_bbox(args, 0)?;
                let new_min = self.min.max(other.min);
                let new_max = self.max.min(other.max);
                if new_min.x < new_max.x && new_min.y < new_max.y && new_min.z < new_max.z {
                    Ok(bbox_value(WritBoundingBox {
                        min: new_min,
                        max: new_max,
                    }))
                } else {
                    Ok(Value::Null)
                }
            }
            "merge" => {
                let other = extract_bbox(args, 0)?;
                Ok(bbox_value(WritBoundingBox {
                    min: self.min.min(other.min),
                    max: self.max.max(other.max),
                }))
            }
            "expand" => {
                let amount = extract_f32(args.first().ok_or("missing amount argument")?, "amount")?;
                Ok(bbox_value(WritBoundingBox {
                    min: self.min - Vec3::splat(amount),
                    max: self.max + Vec3::splat(amount),
                }))
            }
            _ => Err(format!("BoundingBox has no method '{name}'")),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ── Value constructors ──────────────────────────────────────────────

fn rect_value(r: WritRectangle) -> Value {
    Value::Object(Rc::new(RefCell::new(r)))
}

fn bbox_value(b: WritBoundingBox) -> Value {
    Value::Object(Rc::new(RefCell::new(b)))
}

// ── Extraction helpers ──────────────────────────────────────────────

fn extract_rect(args: &[Value], idx: usize) -> Result<WritRectangle, String> {
    let v = args.get(idx).ok_or("missing Rectangle argument")?;
    match v {
        Value::Object(obj) => {
            let b = obj.borrow();
            b.as_any()
                .downcast_ref::<WritRectangle>()
                .cloned()
                .ok_or_else(|| format!("expected Rectangle, got {}", b.type_name()))
        }
        other => Err(format!("expected Rectangle, got {}", other.type_name())),
    }
}

fn extract_bbox(args: &[Value], idx: usize) -> Result<WritBoundingBox, String> {
    let v = args.get(idx).ok_or("missing BoundingBox argument")?;
    match v {
        Value::Object(obj) => {
            let b = obj.borrow();
            b.as_any()
                .downcast_ref::<WritBoundingBox>()
                .cloned()
                .ok_or_else(|| format!("expected BoundingBox, got {}", b.type_name()))
        }
        other => Err(format!("expected BoundingBox, got {}", other.type_name())),
    }
}

// ── Registration ────────────────────────────────────────────────────

pub fn register(vm: &mut VM) {
    vm.register_type("Rectangle", |args| {
        let x = extract_f32(args.first().ok_or("Rectangle: missing x")?, "x")?;
        let y = extract_f32(args.get(1).ok_or("Rectangle: missing y")?, "y")?;
        let w = extract_f32(args.get(2).ok_or("Rectangle: missing width")?, "width")?;
        let h = extract_f32(args.get(3).ok_or("Rectangle: missing height")?, "height")?;
        Ok(Box::new(WritRectangle {
            position: Vec2::new(x, y),
            size: Vec2::new(w, h),
        }))
    });

    vm.register_fn_in_module(
        "Rectangle_fromPoints",
        "rect",
        crate::vm::binding::fn2(|min: Value, max: Value| -> Result<Value, String> {
            let mn = super::vector::value_to_vec2(&min)?;
            let mx = super::vector::value_to_vec2(&max)?;
            Ok(rect_value(WritRectangle {
                position: mn,
                size: mx - mn,
            }))
        }),
    );

    vm.register_type("BoundingBox", |args| {
        let min = super::vector::value_to_vec3(args.first().ok_or("BoundingBox: missing min")?)?;
        let max = super::vector::value_to_vec3(args.get(1).ok_or("BoundingBox: missing max")?)?;
        Ok(Box::new(WritBoundingBox { min, max }))
    });
}
