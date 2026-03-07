use writ_vm::{VM, Value, WritObject};

use crate::vector::extract_f64;

#[derive(Debug)]
struct WritTimer {
    duration: f64,
    elapsed: f64,
    running: bool,
    repeating: bool,
    finished: bool,
    callback: Option<Value>,
}

impl WritTimer {
    fn new(duration: f64) -> Self {
        Self {
            duration,
            elapsed: 0.0,
            running: false,
            repeating: false,
            finished: false,
            callback: None,
        }
    }
}

impl WritObject for WritTimer {
    fn type_name(&self) -> &str {
        "Timer"
    }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        Err(format!("Timer has no field '{}'", name))
    }

    fn set_field(&mut self, name: &str, _value: Value) -> Result<(), String> {
        Err(format!("Timer has no field '{}'", name))
    }

    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "start" => {
                self.running = true;
                self.finished = false;
                Ok(Value::Null)
            }
            "stop" => {
                self.running = false;
                Ok(Value::Null)
            }
            "reset" => {
                self.elapsed = 0.0;
                self.finished = false;
                Ok(Value::Null)
            }
            "update" => {
                let delta = extract_f64(args.first().ok_or("missing delta")?, "delta")?;
                if !self.running || self.finished {
                    return Ok(Value::Null);
                }
                self.elapsed += delta;
                if self.elapsed >= self.duration {
                    self.finished = true;
                    if self.repeating {
                        self.elapsed -= self.duration;
                        self.finished = false;
                    } else {
                        self.running = false;
                    }
                }
                Ok(Value::Null)
            }
            "isFinished" => Ok(Value::Bool(self.finished)),
            "isRunning" => Ok(Value::Bool(self.running)),
            "remaining" => Ok(Value::F64((self.duration - self.elapsed).max(0.0))),
            "elapsed" => Ok(Value::F64(self.elapsed)),
            "setRepeating" => {
                let v = match args.first() {
                    Some(Value::Bool(b)) => *b,
                    _ => return Err("setRepeating expects a bool".into()),
                };
                self.repeating = v;
                Ok(Value::Null)
            }
            "setCallback" => {
                self.callback = args.first().cloned();
                Ok(Value::Null)
            }
            _ => Err(format!("Timer has no method '{}'", name)),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub fn register(vm: &mut VM) {
    vm.register_type("Timer", |args| {
        let duration = extract_f64(args.first().ok_or("Timer: missing duration")?, "duration")?;
        Ok(Box::new(WritTimer::new(duration)))
    });
}
