use std::fmt;
use std::rc::Rc;

use crate::vm::{VM, Value, WritObject};

use super::vector::extract_f64;

#[derive(Clone, Copy, PartialEq)]
enum LoopMode {
    None,
    Loop,
    PingPong,
}

impl fmt::Debug for LoopMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoopMode::None => write!(f, "None"),
            LoopMode::Loop => write!(f, "Loop"),
            LoopMode::PingPong => write!(f, "PingPong"),
        }
    }
}

struct WritTween {
    from: f64,
    to: f64,
    duration: f64,
    elapsed: f64,
    delay: f64,
    loop_mode: LoopMode,
    easing: Option<Rc<dyn Fn(f64) -> f64>>,
    finished: bool,
    direction: f64,
}

impl fmt::Debug for WritTween {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tween")
            .field("from", &self.from)
            .field("to", &self.to)
            .field("duration", &self.duration)
            .field("elapsed", &self.elapsed)
            .field("finished", &self.finished)
            .finish()
    }
}

impl WritTween {
    fn new(from: f64, to: f64, duration: f64) -> Self {
        Self {
            from,
            to,
            duration,
            elapsed: 0.0,
            delay: 0.0,
            loop_mode: LoopMode::None,
            easing: None,
            finished: false,
            direction: 1.0,
        }
    }

    fn raw_t(&self) -> f64 {
        if self.duration <= 0.0 {
            return 1.0;
        }
        ((self.elapsed - self.delay).max(0.0) / self.duration).clamp(0.0, 1.0)
    }

    fn current_value(&self) -> f64 {
        let t = self.raw_t();
        let eased = match &self.easing {
            Some(f) => f(t),
            None => t,
        };
        self.from + (self.to - self.from) * eased
    }
}

impl WritObject for WritTween {
    fn type_name(&self) -> &str {
        "Tween"
    }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        Err(format!("Tween has no field '{}'", name))
    }

    fn set_field(&mut self, name: &str, _value: Value) -> Result<(), String> {
        Err(format!("Tween has no field '{}'", name))
    }

    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "setEasing" => match args.first() {
                Some(Value::Str(s)) => {
                    let f: Rc<dyn Fn(f64) -> f64> = match &**s {
                        "linear" => Rc::new(|t| t),
                        "easeInQuad" => Rc::new(|t| t * t),
                        "easeOutQuad" => Rc::new(|t| 1.0 - (1.0 - t) * (1.0 - t)),
                        "easeInOutQuad" => Rc::new(|t| {
                            if t < 0.5 {
                                2.0 * t * t
                            } else {
                                1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                            }
                        }),
                        "easeInCubic" => Rc::new(|t| t * t * t),
                        "easeOutCubic" => Rc::new(|t| 1.0 - (1.0 - t).powi(3)),
                        "easeInOutCubic" => Rc::new(|t| {
                            if t < 0.5 {
                                4.0 * t * t * t
                            } else {
                                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                            }
                        }),
                        "smoothstep" => Rc::new(|t| t * t * (3.0 - 2.0 * t)),
                        _ => return Err(format!("unknown easing: {}", s)),
                    };
                    self.easing = Some(f);
                    Ok(Value::Null)
                }
                _ => Err("setEasing expects a string easing name".into()),
            },
            "setLoop" => {
                let mode = match args.first() {
                    Some(Value::Str(s)) => match &**s {
                        "none" => LoopMode::None,
                        "loop" => LoopMode::Loop,
                        "pingpong" => LoopMode::PingPong,
                        _ => return Err(format!("unknown loop mode: {}", s)),
                    },
                    _ => return Err("setLoop expects a string".into()),
                };
                self.loop_mode = mode;
                Ok(Value::Null)
            }
            "setDelay" => {
                let d = extract_f64(args.first().ok_or("missing delay")?, "delay")?;
                self.delay = d;
                Ok(Value::Null)
            }
            "update" => {
                let delta = extract_f64(args.first().ok_or("missing delta")?, "delta")?;
                if self.finished {
                    return Ok(Value::F64(self.current_value()));
                }
                self.elapsed += delta * self.direction;

                let active_elapsed = self.elapsed - self.delay;
                if active_elapsed >= self.duration {
                    match self.loop_mode {
                        LoopMode::None => {
                            self.elapsed = self.delay + self.duration;
                            self.finished = true;
                        }
                        LoopMode::Loop => {
                            self.elapsed -= self.duration;
                        }
                        LoopMode::PingPong => {
                            self.direction = -self.direction;
                            self.elapsed = self.delay + self.duration;
                        }
                    }
                } else if active_elapsed < 0.0 && self.loop_mode == LoopMode::PingPong {
                    self.direction = -self.direction;
                    self.elapsed = self.delay;
                }

                Ok(Value::F64(self.current_value()))
            }
            "value" => Ok(Value::F64(self.current_value())),
            "isFinished" => Ok(Value::Bool(self.finished)),
            "reset" => {
                self.elapsed = 0.0;
                self.finished = false;
                self.direction = 1.0;
                Ok(Value::Null)
            }
            _ => Err(format!("Tween has no method '{}'", name)),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub fn register(vm: &mut VM) {
    vm.register_type("Tween", |args| {
        let from = extract_f64(args.first().ok_or("Tween: missing from")?, "from")?;
        let to = extract_f64(args.get(1).ok_or("Tween: missing to")?, "to")?;
        let duration = extract_f64(args.get(2).ok_or("Tween: missing duration")?, "duration")?;
        Ok(Box::new(WritTween::new(from, to, duration)))
    });
}
