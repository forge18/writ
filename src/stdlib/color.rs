use std::cell::RefCell;
use std::rc::Rc;

use crate::vm::{VM, Value, WritObject};

use super::vector::extract_f32;

#[derive(Debug, Clone)]
pub struct WritColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl WritObject for WritColor {
    fn type_name(&self) -> &str {
        "Color"
    }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        match name {
            "r" => Ok(Value::F64(self.r as f64)),
            "g" => Ok(Value::F64(self.g as f64)),
            "b" => Ok(Value::F64(self.b as f64)),
            "a" => Ok(Value::F64(self.a as f64)),
            _ => Err(format!("Color has no field '{name}'")),
        }
    }

    fn set_field(&mut self, name: &str, value: Value) -> Result<(), String> {
        let f = extract_f32(&value, name)?;
        match name {
            "r" => self.r = f,
            "g" => self.g = f,
            "b" => self.b = f,
            "a" => self.a = f,
            _ => return Err(format!("Color has no field '{name}'")),
        }
        Ok(())
    }

    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "toHex" => {
                let r = (self.r.clamp(0.0, 1.0) * 255.0) as u8;
                let g = (self.g.clamp(0.0, 1.0) * 255.0) as u8;
                let b = (self.b.clamp(0.0, 1.0) * 255.0) as u8;
                let a = (self.a.clamp(0.0, 1.0) * 255.0) as u8;
                if a == 255 {
                    Ok(Value::Str(Rc::new(format!("#{r:02X}{g:02X}{b:02X}"))))
                } else {
                    Ok(Value::Str(Rc::new(format!(
                        "#{r:02X}{g:02X}{b:02X}{a:02X}"
                    ))))
                }
            }
            "toHSV" => {
                let (h, s, v) = rgb_to_hsv(self.r, self.g, self.b);
                let arr = vec![
                    Value::F64(h as f64),
                    Value::F64(s as f64),
                    Value::F64(v as f64),
                ];
                Ok(Value::Array(Rc::new(RefCell::new(arr))))
            }
            "lerp" => {
                let other = extract_color(args, 0)?;
                let t = extract_f32(args.get(1).ok_or("missing argument 't'")?, "t")?;
                Ok(color_value(WritColor {
                    r: self.r + (other.r - self.r) * t,
                    g: self.g + (other.g - self.g) * t,
                    b: self.b + (other.b - self.b) * t,
                    a: self.a + (other.a - self.a) * t,
                }))
            }
            "lighten" => {
                let amount = extract_f32(args.first().ok_or("missing amount argument")?, "amount")?;
                Ok(color_value(WritColor {
                    r: (self.r + amount).min(1.0),
                    g: (self.g + amount).min(1.0),
                    b: (self.b + amount).min(1.0),
                    a: self.a,
                }))
            }
            "darken" => {
                let amount = extract_f32(args.first().ok_or("missing amount argument")?, "amount")?;
                Ok(color_value(WritColor {
                    r: (self.r - amount).max(0.0),
                    g: (self.g - amount).max(0.0),
                    b: (self.b - amount).max(0.0),
                    a: self.a,
                }))
            }
            "inverted" => Ok(color_value(WritColor {
                r: 1.0 - self.r,
                g: 1.0 - self.g,
                b: 1.0 - self.b,
                a: self.a,
            })),
            "withAlpha" => {
                let a = extract_f32(args.first().ok_or("missing alpha argument")?, "alpha")?;
                Ok(color_value(WritColor {
                    r: self.r,
                    g: self.g,
                    b: self.b,
                    a,
                }))
            }
            _ => Err(format!("Color has no method '{name}'")),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

fn color_value(c: WritColor) -> Value {
    Value::Object(Rc::new(RefCell::new(c)))
}

fn extract_color(args: &[Value], idx: usize) -> Result<WritColor, String> {
    let v = args.get(idx).ok_or("missing Color argument")?;
    match v {
        Value::Object(obj) => {
            let b = obj.borrow();
            b.as_any()
                .downcast_ref::<WritColor>()
                .cloned()
                .ok_or_else(|| format!("expected Color, got {}", b.type_name()))
        }
        other => Err(format!("expected Color, got {}", other.type_name())),
    }
}

fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };

    let s = if max == 0.0 { 0.0 } else { delta / max };
    (if h < 0.0 { h + 360.0 } else { h }, s, max)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (r + m, g + m, b + m)
}

fn parse_hex(s: &str) -> Result<WritColor, String> {
    let s = s.trim_start_matches('#');
    match s.len() {
        6 => {
            let r = u8::from_str_radix(&s[0..2], 16).map_err(|e| e.to_string())? as f32 / 255.0;
            let g = u8::from_str_radix(&s[2..4], 16).map_err(|e| e.to_string())? as f32 / 255.0;
            let b = u8::from_str_radix(&s[4..6], 16).map_err(|e| e.to_string())? as f32 / 255.0;
            Ok(WritColor { r, g, b, a: 1.0 })
        }
        8 => {
            let r = u8::from_str_radix(&s[0..2], 16).map_err(|e| e.to_string())? as f32 / 255.0;
            let g = u8::from_str_radix(&s[2..4], 16).map_err(|e| e.to_string())? as f32 / 255.0;
            let b = u8::from_str_radix(&s[4..6], 16).map_err(|e| e.to_string())? as f32 / 255.0;
            let a = u8::from_str_radix(&s[6..8], 16).map_err(|e| e.to_string())? as f32 / 255.0;
            Ok(WritColor { r, g, b, a })
        }
        _ => Err(format!("invalid hex color: #{s}")),
    }
}

// ── Registration ────────────────────────────────────────────────────

pub fn register(vm: &mut VM) {
    // Color(r, g, b) or Color(r, g, b, a)
    vm.register_type("Color", |args| {
        let r = extract_f32(args.first().ok_or("Color: missing r")?, "r")?;
        let g = extract_f32(args.get(1).ok_or("Color: missing g")?, "g")?;
        let b = extract_f32(args.get(2).ok_or("Color: missing b")?, "b")?;
        let a = args
            .get(3)
            .map(|v| extract_f32(v, "a"))
            .transpose()?
            .unwrap_or(1.0);
        Ok(Box::new(WritColor { r, g, b, a }))
    });

    // Color.fromHex
    vm.register_fn_in_module(
        "Color_fromHex",
        "color",
        crate::vm::binding::fn1(|hex: String| -> Result<Value, String> {
            let c = parse_hex(&hex)?;
            Ok(color_value(c))
        }),
    );

    // Color.fromHSV
    vm.register_fn_in_module(
        "Color_fromHSV",
        "color",
        crate::vm::binding::fn3(|h: f64, s: f64, v: f64| -> Result<Value, String> {
            let (r, g, b) = hsv_to_rgb(h as f32, s as f32, v as f32);
            Ok(color_value(WritColor { r, g, b, a: 1.0 }))
        }),
    );

    // Constants
    vm.register_global(
        "Color_WHITE",
        color_value(WritColor {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        }),
    );
    vm.register_global(
        "Color_BLACK",
        color_value(WritColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        }),
    );
    vm.register_global(
        "Color_RED",
        color_value(WritColor {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        }),
    );
    vm.register_global(
        "Color_GREEN",
        color_value(WritColor {
            r: 0.0,
            g: 1.0,
            b: 0.0,
            a: 1.0,
        }),
    );
    vm.register_global(
        "Color_BLUE",
        color_value(WritColor {
            r: 0.0,
            g: 0.0,
            b: 1.0,
            a: 1.0,
        }),
    );
    vm.register_global(
        "Color_YELLOW",
        color_value(WritColor {
            r: 1.0,
            g: 1.0,
            b: 0.0,
            a: 1.0,
        }),
    );
    vm.register_global(
        "Color_CYAN",
        color_value(WritColor {
            r: 0.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        }),
    );
    vm.register_global(
        "Color_MAGENTA",
        color_value(WritColor {
            r: 1.0,
            g: 0.0,
            b: 1.0,
            a: 1.0,
        }),
    );
    vm.register_global(
        "Color_TRANSPARENT",
        color_value(WritColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        }),
    );
}
