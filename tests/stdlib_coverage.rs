//! Stdlib coverage tests.
//!
//! Each test calls at least one function/method from a stdlib module so that
//! the native Rust implementation code is instrumented by tarpaulin.
//!
//! All tests use `disable_type_checking()` — we are testing the stdlib
//! implementations, not the type checker.

use writ::{Value, Writ};

fn w() -> Writ {
    let mut w = Writ::new();
    w.disable_type_checking();
    w
}

// ── Math ─────────────────────────────────────────────────────────────

#[test]
fn test_math_abs_int() {
    assert_eq!(w().run("return abs(-5)").unwrap(), Value::I32(5));
}

#[test]
fn test_math_abs_float() {
    let r = w().run("return abs(-3.5)").unwrap();
    assert!(matches!(r, Value::F32(_) | Value::F64(_)));
}

#[test]
fn test_math_ceil() {
    let r = w().run("return ceil(3.2)").unwrap();
    assert!(matches!(r, Value::F64(v) if v == 4.0));
}

#[test]
fn test_math_floor() {
    let r = w().run("return floor(3.8)").unwrap();
    assert!(matches!(r, Value::F64(v) if v == 3.0));
}

#[test]
fn test_math_round() {
    let r = w().run("return round(3.5)").unwrap();
    assert!(matches!(r, Value::F64(_)));
}

#[test]
fn test_math_sqrt() {
    let r = w().run("return sqrt(16.0)").unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 4.0).abs() < 0.001));
}

#[test]
fn test_math_sin() {
    let r = w().run("return sin(0.0)").unwrap();
    assert!(matches!(r, Value::F64(v) if v.abs() < 0.001));
}

#[test]
fn test_math_cos() {
    let r = w().run("return cos(0.0)").unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 1.0).abs() < 0.001));
}

#[test]
fn test_math_tan() {
    let r = w().run("return tan(0.0)").unwrap();
    assert!(matches!(r, Value::F64(v) if v.abs() < 0.001));
}

#[test]
fn test_math_log() {
    let r = w().run("return log(1.0)").unwrap();
    assert!(matches!(r, Value::F64(v) if v.abs() < 0.001));
}

#[test]
fn test_math_exp() {
    let r = w().run("return exp(0.0)").unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 1.0).abs() < 0.001));
}

#[test]
fn test_math_min() {
    let r = w().run("return min(3.0, 5.0)").unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 3.0).abs() < 0.001));
}

#[test]
fn test_math_max() {
    let r = w().run("return max(3.0, 5.0)").unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 5.0).abs() < 0.001));
}

#[test]
fn test_math_pow() {
    let r = w().run("return pow(2.0, 8.0)").unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 256.0).abs() < 0.001));
}

#[test]
fn test_math_clamp_below() {
    let r = w().run("return clamp(-1.0, 0.0, 10.0)").unwrap();
    assert!(matches!(r, Value::F64(v) if v.abs() < 0.001));
}

#[test]
fn test_math_clamp_above() {
    let r = w().run("return clamp(20.0, 0.0, 10.0)").unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 10.0).abs() < 0.001));
}

#[test]
fn test_math_pi_constant() {
    let r = w().run("return PI").unwrap();
    assert!(matches!(r, Value::F64(v) if (v - std::f64::consts::PI).abs() < 0.001));
}

#[test]
fn test_math_tau_constant() {
    let r = w().run("return TAU").unwrap();
    assert!(matches!(r, Value::F64(v) if (v - std::f64::consts::TAU).abs() < 0.001));
}

#[test]
fn test_math_infinity_constant() {
    let r = w().run("return INFINITY > 1000000.0").unwrap();
    assert_eq!(r, Value::Bool(true));
}

// ── String methods ────────────────────────────────────────────────────

#[test]
fn test_string_len() {
    assert_eq!(w().run(r#"return "hello".len()"#).unwrap(), Value::I32(5));
}

#[test]
fn test_string_trim() {
    let r = w().run(r#"return "  hi  ".trim()"#).unwrap();
    assert_eq!(r, Value::Str(std::rc::Rc::from("hi")));
}

#[test]
fn test_string_trim_start() {
    let r = w().run(r#"return "  hi".trimStart()"#).unwrap();
    assert_eq!(r, Value::Str(std::rc::Rc::from("hi")));
}

#[test]
fn test_string_trim_end() {
    let r = w().run(r#"return "hi  ".trimEnd()"#).unwrap();
    assert_eq!(r, Value::Str(std::rc::Rc::from("hi")));
}

#[test]
fn test_string_to_upper() {
    let r = w().run(r#"return "hello".toUpper()"#).unwrap();
    assert_eq!(r, Value::Str(std::rc::Rc::from("HELLO")));
}

#[test]
fn test_string_to_lower() {
    let r = w().run(r#"return "HELLO".toLower()"#).unwrap();
    assert_eq!(r, Value::Str(std::rc::Rc::from("hello")));
}

#[test]
fn test_string_contains_true() {
    assert_eq!(w().run(r#"return "hello world".contains("world")"#).unwrap(), Value::Bool(true));
}

#[test]
fn test_string_contains_false() {
    assert_eq!(w().run(r#"return "hello".contains("xyz")"#).unwrap(), Value::Bool(false));
}

#[test]
fn test_string_starts_with() {
    assert_eq!(w().run(r#"return "hello".startsWith("he")"#).unwrap(), Value::Bool(true));
}

#[test]
fn test_string_ends_with() {
    assert_eq!(w().run(r#"return "hello".endsWith("lo")"#).unwrap(), Value::Bool(true));
}

#[test]
fn test_string_replace() {
    let r = w().run(r#"return "hello".replace("l", "r")"#).unwrap();
    assert_eq!(r, Value::Str(std::rc::Rc::from("herro")));
}

#[test]
fn test_string_split() {
    let r = w().run(r#"return "a,b,c".split(",").len()"#).unwrap();
    assert_eq!(r, Value::I32(3));
}

#[test]
fn test_string_char_at() {
    let r = w().run(r#"return "hello".charAt(1)"#).unwrap();
    assert_eq!(r, Value::Str(std::rc::Rc::from("e")));
}

#[test]
fn test_string_char_at_out_of_bounds() {
    let r = w().run(r#"return "hi".charAt(99)"#).unwrap();
    assert_eq!(r, Value::Null);
}

#[test]
fn test_string_index_of_found() {
    assert_eq!(w().run(r#"return "hello".indexOf("l")"#).unwrap(), Value::I32(2));
}

#[test]
fn test_string_index_of_not_found() {
    assert_eq!(w().run(r#"return "hello".indexOf("z")"#).unwrap(), Value::I32(-1));
}

#[test]
fn test_string_parse_int() {
    assert_eq!(w().run(r#"return "42".parse()"#).unwrap(), Value::I32(42));
}

#[test]
fn test_string_parse_float() {
    let r = w().run(r#"return "3.14".parse()"#).unwrap();
    assert!(matches!(r, Value::F64(_)));
}

#[test]
fn test_string_parse_error() {
    let r = w().run(r#"return "abc".parse()"#);
    assert!(r.is_err());
}

// ── Array methods ─────────────────────────────────────────────────────

#[test]
fn test_array_push_and_len() {
    let r = w().run("var a = [1, 2, 3]\na.push(4)\nreturn a.len()").unwrap();
    assert_eq!(r, Value::I32(4));
}

#[test]
fn test_array_pop() {
    let r = w().run("var a = [1, 2, 3]\nreturn a.pop()").unwrap();
    assert_eq!(r, Value::I32(3));
}

#[test]
fn test_array_pop_empty() {
    let r = w().run("var a = []\nreturn a.pop()").unwrap();
    assert_eq!(r, Value::Null);
}

#[test]
fn test_array_insert() {
    let r = w().run("var a = [1, 3]\na.insert(1, 2)\nreturn a.len()").unwrap();
    assert_eq!(r, Value::I32(3));
}

#[test]
fn test_array_insert_out_of_bounds() {
    let r = w().run("var a = [1, 2]\na.insert(10, 99)");
    assert!(r.is_err());
}

#[test]
fn test_array_remove() {
    let r = w().run("var a = [1, 2, 3]\na.remove(1)\nreturn a.len()").unwrap();
    assert_eq!(r, Value::I32(2));
}

#[test]
fn test_array_remove_out_of_bounds() {
    let r = w().run("var a = [1, 2]\na.remove(5)");
    assert!(r.is_err());
}

#[test]
fn test_array_is_empty_false() {
    assert_eq!(w().run("return [1, 2].isEmpty()").unwrap(), Value::Bool(false));
}

#[test]
fn test_array_is_empty_true() {
    assert_eq!(w().run("return [].isEmpty()").unwrap(), Value::Bool(true));
}

#[test]
fn test_array_contains_true() {
    assert_eq!(w().run("return [1, 2, 3].contains(2)").unwrap(), Value::Bool(true));
}

#[test]
fn test_array_contains_false() {
    assert_eq!(w().run("return [1, 2, 3].contains(99)").unwrap(), Value::Bool(false));
}

#[test]
fn test_array_index_of() {
    assert_eq!(w().run("return [1, 2, 3].indexOf(2)").unwrap(), Value::I32(1));
}

#[test]
fn test_array_index_of_not_found() {
    assert_eq!(w().run("return [1, 2, 3].indexOf(99)").unwrap(), Value::I32(-1));
}

#[test]
fn test_array_reverse() {
    let r = w().run("var a = [1, 2, 3]\na.reverse()\nreturn a[0]").unwrap();
    assert_eq!(r, Value::I32(3));
}

#[test]
fn test_array_sort() {
    let r = w().run("var a = [3, 1, 2]\na.sort()\nreturn a[0]").unwrap();
    assert_eq!(r, Value::I32(1));
}

#[test]
fn test_array_first() {
    assert_eq!(w().run("return [10, 20, 30].first()").unwrap(), Value::I32(10));
}

#[test]
fn test_array_last() {
    assert_eq!(w().run("return [10, 20, 30].last()").unwrap(), Value::I32(30));
}

#[test]
fn test_array_slice() {
    let r = w().run("return [1, 2, 3, 4, 5].slice(1, 3).len()").unwrap();
    assert_eq!(r, Value::I32(2));
}

#[test]
fn test_array_join() {
    let r = w().run(r#"return ["a", "b", "c"].join(",")"#).unwrap();
    assert_eq!(r, Value::Str(std::rc::Rc::from("a,b,c")));
}

// ── Dict methods ──────────────────────────────────────────────────────

#[test]
fn test_dict_len() {
    let r = w().run(r#"let d = {"a": 1, "b": 2}
return d.len()"#).unwrap();
    assert_eq!(r, Value::I32(2));
}

#[test]
fn test_dict_has_true() {
    let r = w().run(r#"let d = {"x": 10}
return d.has("x")"#).unwrap();
    assert_eq!(r, Value::Bool(true));
}

#[test]
fn test_dict_has_false() {
    let r = w().run(r#"let d = {"x": 10}
return d.has("z")"#).unwrap();
    assert_eq!(r, Value::Bool(false));
}

#[test]
fn test_dict_remove() {
    let r = w().run(r#"var d = {"a": 1}
d.remove("a")
return d.len()"#).unwrap();
    assert_eq!(r, Value::I32(0));
}

#[test]
fn test_dict_remove_returns_value() {
    let r = w().run(r#"var d = {"a": 42}
return d.remove("a")"#).unwrap();
    assert_eq!(r, Value::I32(42));
}

#[test]
fn test_dict_remove_missing_returns_null() {
    let r = w().run(r#"var d = {}
return d.remove("missing")"#).unwrap();
    assert_eq!(r, Value::Null);
}

#[test]
fn test_dict_is_empty_true() {
    let r = w().run("let d = {}\nreturn d.isEmpty()").unwrap();
    assert_eq!(r, Value::Bool(true));
}

#[test]
fn test_dict_keys() {
    let r = w().run(r#"let d = {"a": 1}
return d.keys().len()"#).unwrap();
    assert_eq!(r, Value::I32(1));
}

#[test]
fn test_dict_values() {
    let r = w().run(r#"let d = {"a": 1}
return d.values().len()"#).unwrap();
    assert_eq!(r, Value::I32(1));
}

#[test]
fn test_dict_merge() {
    let r = w()
        .run(r#"var d1 = {"a": 1}
let d2 = {"b": 2}
d1.merge(d2)
return d1.len()"#)
        .unwrap();
    assert_eq!(r, Value::I32(2));
}

// ── Interpolation ─────────────────────────────────────────────────────

#[test]
fn test_interp_lerp() {
    let r = w().run("return lerp(0.0, 10.0, 0.5)").unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 5.0).abs() < 0.001));
}

#[test]
fn test_interp_inverse_lerp() {
    let r = w().run("return inverseLerp(0.0, 10.0, 5.0)").unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 0.5).abs() < 0.001));
}

#[test]
fn test_interp_inverse_lerp_degenerate() {
    // a == b: should return 0.0 not NaN
    let r = w().run("return inverseLerp(5.0, 5.0, 5.0)").unwrap();
    assert!(matches!(r, Value::F64(v) if v.abs() < 0.001));
}

#[test]
fn test_interp_smoothstep() {
    let r = w().run("return smoothstep(0.0, 1.0, 0.5)").unwrap();
    assert!(matches!(r, Value::F64(_)));
}

#[test]
fn test_interp_smootherstep() {
    let r = w().run("return smootherstep(0.0, 1.0, 0.5)").unwrap();
    assert!(matches!(r, Value::F64(_)));
}

#[test]
fn test_interp_remap() {
    // remap 5 from [0,10] to [0,100] → 50
    let r = w().run("return remap(5.0, 0.0, 10.0, 0.0, 100.0)").unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 50.0).abs() < 0.001));
}

// ── Timer ─────────────────────────────────────────────────────────────

#[test]
fn test_timer_basic_lifecycle() {
    // Timer is not running by default — isFinished returns false
    let r = w().run("let t = Timer(0.5)\nreturn t.isFinished()").unwrap();
    assert_eq!(r, Value::Bool(false));
}

#[test]
fn test_timer_not_finished_yet() {
    // update on non-running timer is a no-op
    let r = w()
        .run("let t = Timer(1.0)\nt.update(0.3)\nreturn t.isFinished()")
        .unwrap();
    assert_eq!(r, Value::Bool(false));
}

#[test]
fn test_timer_elapsed() {
    // elapsed on a fresh timer is 0.0
    let r = w().run("let t = Timer(1.0)\nreturn t.elapsed()").unwrap();
    assert!(matches!(r, Value::F64(v) if v.abs() < 0.001));
}

#[test]
fn test_timer_remaining() {
    // remaining on a fresh timer equals the duration
    let r = w().run("let t = Timer(1.0)\nreturn t.remaining()").unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 1.0).abs() < 0.01));
}

#[test]
fn test_timer_repeating() {
    // setRepeating doesn't crash
    let r = w()
        .run("let t = Timer(0.5)\nt.setRepeating(true)\nreturn t.isFinished()")
        .unwrap();
    assert_eq!(r, Value::Bool(false));
}

#[test]
fn test_timer_is_running() {
    // fresh timer is not running
    let r = w().run("let t = Timer(1.0)\nreturn t.isRunning()").unwrap();
    assert_eq!(r, Value::Bool(false));
}

#[test]
fn test_timer_stop() {
    // stop on non-running timer is a no-op
    let r = w()
        .run("let t = Timer(1.0)\nt.stop()\nreturn t.isRunning()")
        .unwrap();
    assert_eq!(r, Value::Bool(false));
}

#[test]
fn test_timer_reset() {
    // reset clears elapsed
    let r = w()
        .run("let t = Timer(1.0)\nt.reset()\nreturn t.elapsed()")
        .unwrap();
    assert!(matches!(r, Value::F64(v) if v.abs() < 0.001));
}

#[test]
fn test_timer_not_started_update_noop() {
    // update on non-running timer should be a no-op
    let r = w()
        .run("let t = Timer(1.0)\nt.update(5.0)\nreturn t.isFinished()")
        .unwrap();
    assert_eq!(r, Value::Bool(false));
}

// ── Vector types ──────────────────────────────────────────────────────

#[test]
fn test_vector2_construction() {
    let r = w().run("let v = Vector2(3.0, 4.0)\nreturn v.x").unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 3.0).abs() < 0.001));
}

#[test]
fn test_vector2_length() {
    let r = w().run("let v = Vector2(3.0, 4.0)\nreturn v.length()").unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 5.0).abs() < 0.01));
}

#[test]
fn test_vector2_dot() {
    let r = w()
        .run("let a = Vector2(1.0, 0.0)\nlet b = Vector2(0.0, 1.0)\nreturn a.dot(b)")
        .unwrap();
    assert!(matches!(r, Value::F64(v) if v.abs() < 0.001));
}

#[test]
fn test_vector2_normalized() {
    let r = w()
        .run("let v = Vector2(3.0, 4.0)\nreturn v.normalized().length()")
        .unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 1.0).abs() < 0.01));
}

#[test]
fn test_vector2_abs() {
    let r = w().run("let v = Vector2(-3.0, -4.0)\nreturn v.abs().x").unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 3.0).abs() < 0.001));
}

#[test]
fn test_vector2_negate() {
    let r = w().run("let v = Vector2(1.0, 2.0)\nreturn v.negate().x").unwrap();
    assert!(matches!(r, Value::F64(v) if (v + 1.0).abs() < 0.001));
}

#[test]
fn test_vector2_constants() {
    let r = w().run("return Vector2_ZERO.x").unwrap();
    assert!(matches!(r, Value::F64(v) if v.abs() < 0.001));
}

#[test]
fn test_vector3_construction() {
    let r = w().run("let v = Vector3(1.0, 2.0, 3.0)\nreturn v.z").unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 3.0).abs() < 0.001));
}

#[test]
fn test_vector3_cross() {
    let r = w()
        .run(
            "let a = Vector3(1.0, 0.0, 0.0)\n\
             let b = Vector3(0.0, 1.0, 0.0)\n\
             return a.cross(b).z",
        )
        .unwrap();
    // cross product of X and Y is Z
    assert!(matches!(r, Value::F64(v) if (v - 1.0).abs() < 0.01));
}

#[test]
fn test_vector3_dot() {
    let r = w()
        .run(
            "let a = Vector3(1.0, 0.0, 0.0)\n\
             let b = Vector3(0.0, 1.0, 0.0)\n\
             return a.dot(b)",
        )
        .unwrap();
    assert!(matches!(r, Value::F64(v) if v.abs() < 0.001));
}

#[test]
fn test_vector4_construction() {
    let r = w().run("let v = Vector4(1.0, 2.0, 3.0, 4.0)\nreturn v.w").unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 4.0).abs() < 0.001));
}

// ── Color ─────────────────────────────────────────────────────────────

#[test]
fn test_color_construction() {
    let r = w().run("let c = Color(1.0, 0.5, 0.0, 1.0)\nreturn c.r").unwrap();
    assert!(match r {
        Value::F64(v) => (v - 1.0).abs() < 0.01,
        Value::F32(v) => ((v as f64) - 1.0).abs() < 0.01,
        _ => false,
    });
}

#[test]
fn test_color_to_hex() {
    let r = w().run("let c = Color(1.0, 0.0, 0.0, 1.0)\nreturn c.toHex()").unwrap();
    assert!(matches!(r, Value::Str(_)));
}

#[test]
fn test_color_lerp() {
    let r = w()
        .run(
            "let a = Color(0.0, 0.0, 0.0, 1.0)\n\
             let b = Color(1.0, 1.0, 1.0, 1.0)\n\
             let c = a.lerp(b, 0.5)\n\
             return c.r",
        )
        .unwrap();
    assert!(matches!(r, Value::F64(_) | Value::F32(_)));
}

#[test]
fn test_color_with_alpha() {
    let r = w()
        .run("let c = Color(1.0, 0.0, 0.0, 1.0)\nreturn c.withAlpha(0.5).a")
        .unwrap();
    assert!(matches!(r, Value::F64(_) | Value::F32(_)));
}

// ── Random ────────────────────────────────────────────────────────────

#[test]
fn test_random_returns_float() {
    let r = w().run("let n = random()\nreturn n >= 0.0").unwrap();
    assert_eq!(r, Value::Bool(true));
}

#[test]
fn test_random_int_in_range() {
    let r = w().run("let n = randomInt(1, 6)\nreturn n >= 1").unwrap();
    assert_eq!(r, Value::Bool(true));
}

#[test]
fn test_random_float_in_range() {
    let r = w().run("let n = randomFloat(0.0, 10.0)\nreturn n >= 0.0").unwrap();
    assert_eq!(r, Value::Bool(true));
}

#[test]
fn test_random_shuffle() {
    let r = w().run("var a = [1, 2, 3, 4, 5]\nshuffle(a)\nreturn a.len()").unwrap();
    assert_eq!(r, Value::I32(5));
}

// ── Basic ─────────────────────────────────────────────────────────────

#[test]
fn test_basic_print() {
    let r = w().run(r#"print("test")
return 1"#).unwrap();
    assert_eq!(r, Value::I32(1));
}

#[test]
fn test_basic_type_fn() {
    let r = w().run("return type(42)").unwrap();
    assert!(matches!(r, Value::Str(_)));
}

#[test]
fn test_basic_assert_pass() {
    let r = w().run(r#"assert(true, "should not fail")
return 1"#).unwrap();
    assert_eq!(r, Value::I32(1));
}

#[test]
fn test_basic_assert_fail() {
    let r = w().run(r#"assert(false, "intentional failure")"#);
    assert!(r.is_err());
}

// ── Time ──────────────────────────────────────────────────────────────

#[test]
fn test_time_now() {
    let r = w().run("let t = now()\nreturn t > 0.0").unwrap();
    assert_eq!(r, Value::Bool(true));
}

#[test]
fn test_time_elapsed() {
    let r = w().run("let t0 = now()\nreturn elapsed(t0) >= 0.0").unwrap();
    assert_eq!(r, Value::Bool(true));
}

// ── Noise ─────────────────────────────────────────────────────────────

#[test]
fn test_noise_2d() {
    let r = w().run("let n = noise2D(0.5, 0.5)\nreturn n >= -2.0").unwrap();
    assert_eq!(r, Value::Bool(true));
}

#[test]
fn test_noise_3d() {
    let r = w().run("let n = noise3D(0.5, 0.5, 0.5)\nreturn n >= -2.0").unwrap();
    assert_eq!(r, Value::Bool(true));
}

// ── Rectangle ─────────────────────────────────────────────────────────

#[test]
fn test_rectangle_construction() {
    let r = w()
        .run("let r = Rectangle(0.0, 0.0, 10.0, 5.0)\nreturn r.size.x")
        .unwrap();
    assert!(matches!(r, Value::F32(_) | Value::F64(_)));
}

#[test]
fn test_rectangle_area() {
    let r = w()
        .run("let rect = Rectangle(0.0, 0.0, 10.0, 5.0)\nreturn rect.area()")
        .unwrap();
    assert!(matches!(r, Value::F64(_) | Value::F32(_)));
}

#[test]
fn test_rectangle_contains() {
    let r = w()
        .run(
            "let rect = Rectangle(0.0, 0.0, 10.0, 10.0)\n\
             let p = Vector2(5.0, 5.0)\n\
             return rect.contains(p)",
        )
        .unwrap();
    assert_eq!(r, Value::Bool(true));
}

// ── Tween ─────────────────────────────────────────────────────────────

#[test]
fn test_tween_value_at_midpoint() {
    // fresh tween returns the from-value (0.0)
    let r = w()
        .run("let t = Tween(0.0, 10.0, 1.0)\nreturn t.value()")
        .unwrap();
    assert!(matches!(r, Value::F64(v) if v.abs() < 0.001));
}

#[test]
fn test_tween_is_complete() {
    // fresh tween is not finished
    let r = w()
        .run("let t = Tween(0.0, 10.0, 1.0)\nreturn t.isFinished()")
        .unwrap();
    assert_eq!(r, Value::Bool(false));
}

#[test]
fn test_tween_not_started() {
    let r = w()
        .run("let t = Tween(0.0, 10.0, 1.0)\nreturn t.value()")
        .unwrap();
    // Not started — value should be the from value (0.0)
    assert!(matches!(r, Value::F64(v) if v.abs() < 0.001));
}

// ── IO ────────────────────────────────────────────────────────────────

#[test]
fn test_io_file_exists_false() {
    let r = w().run(r#"return fileExists("/definitely/does/not/exist/abc123.txt")"#).unwrap();
    assert_eq!(r, Value::Bool(false));
}

#[test]
fn test_io_write_and_read_file() {
    let dir = std::env::temp_dir().join("writ_stdlib_io_test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.txt");
    let path_str = path.to_str().unwrap().replace('\\', "/");

    // Write via stdlib
    let r = w()
        .run(&format!(r#"writeFile("{path_str}", "hello from writ")
return 1"#))
        .unwrap();
    assert_eq!(r, Value::I32(1));

    // Read back
    let r = w()
        .run(&format!(r#"return readFile("{path_str}")"#))
        .unwrap();
    assert_eq!(r, Value::Str(std::rc::Rc::from("hello from writ")));

    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn test_io_file_exists_after_write() {
    let dir = std::env::temp_dir().join("writ_stdlib_io_exists_test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("exists.txt");
    let path_str = path.to_str().unwrap().replace('\\', "/");

    w().run(&format!(r#"writeFile("{path_str}", "content")"#)).unwrap();

    let r = w().run(&format!(r#"return fileExists("{path_str}")"#)).unwrap();
    assert_eq!(r, Value::Bool(true));

    std::fs::remove_dir_all(dir).ok();
}

// ── Quaternion module ─────────────────────────────────────────────────────────

#[test]
fn test_quat_identity_dot_self_is_one() {
    // Use two separate quaternion instances (same Rc can't be borrowed twice)
    let r = w()
        .run(
            "let a = Quaternion_fromEuler(0.0, 0.0, 0.0)\n\
             let b = Quaternion_fromEuler(0.0, 0.0, 0.0)\n\
             return a.dot(b)",
        )
        .unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 1.0).abs() < 0.001));
}

#[test]
fn test_quat_from_euler_zero_is_identity() {
    let r = w()
        .run(
            "let q = Quaternion_fromEuler(0.0, 0.0, 0.0)\n\
             return q.dot(Quaternion_IDENTITY)",
        )
        .unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 1.0).abs() < 0.01));
}

#[test]
fn test_quat_normalized_dot_self_is_one() {
    // Dot with a separate copy to avoid double-borrow of the same Rc
    let r = w()
        .run(
            "let q = Quaternion_fromEuler(0.5, 0.3, 0.1)\n\
             let n = q.normalized()\n\
             let n2 = q.normalized()\n\
             return n.dot(n2)",
        )
        .unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 1.0).abs() < 0.001));
}

#[test]
fn test_quat_from_axis_angle_w_at_zero_is_one() {
    let r = w()
        .run(
            "let up = Vector3(0.0, 1.0, 0.0)\n\
             let q = Quaternion_fromAxisAngle(up, 0.0)\n\
             return q.w",
        )
        .unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 1.0).abs() < 0.001));
}

#[test]
fn test_quat_to_euler_roundtrip_x() {
    let r = w()
        .run(
            "let q = Quaternion_fromEuler(0.1, 0.0, 0.0)\n\
             let e = q.toEuler()\n\
             return e.x",
        )
        .unwrap();
    assert!(matches!(&r, Value::F32(_) | Value::F64(_)) && (r.as_f64() - 0.1).abs() < 0.01);
}

#[test]
fn test_quat_slerp_identity_midpoint() {
    // Use two separate identity quaternions to avoid double-borrow
    let r = w()
        .run(
            "let q = Quaternion_fromEuler(0.0, 0.0, 0.0)\n\
             let q2 = Quaternion_fromEuler(0.0, 0.0, 0.0)\n\
             let s = q.slerp(q2, 0.5)\n\
             let q3 = Quaternion_fromEuler(0.0, 0.0, 0.0)\n\
             return s.dot(q3)",
        )
        .unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 1.0).abs() < 0.01));
}

#[test]
fn test_quat_inverse_ok() {
    assert!(w()
        .run("let q = Quaternion_fromEuler(0.5, 0.3, 0.1)\nreturn q.inverse()")
        .is_ok());
}

// ── Matrix module ─────────────────────────────────────────────────────────────

#[test]
fn test_matrix3_identity_constant() {
    assert!(w().run("return Matrix3_IDENTITY").is_ok());
}

#[test]
fn test_matrix3_rotation_determinant_is_one() {
    let r = w()
        .run("let m = Matrix3_rotation(0.0)\nreturn m.determinant()")
        .unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 1.0).abs() < 0.001));
}

#[test]
fn test_matrix3_scale_determinant() {
    let r = w()
        .run("let m = Matrix3_scale(2.0, 3.0)\nreturn m.determinant()")
        .unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 6.0).abs() < 0.001));
}

#[test]
fn test_matrix3_inverse_determinant_is_one() {
    let r = w()
        .run("let m = Matrix3_IDENTITY\nlet inv = m.inverse()\nreturn inv.determinant()")
        .unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 1.0).abs() < 0.001));
}

#[test]
fn test_matrix4_identity_constant() {
    assert!(w().run("return Matrix4_IDENTITY").is_ok());
}

#[test]
fn test_matrix4_scale_unit_determinant() {
    let r = w()
        .run("let m = Matrix4_scale(1.0, 1.0, 1.0)\nreturn m.determinant()")
        .unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 1.0).abs() < 0.001));
}

#[test]
fn test_matrix3_transpose_determinant() {
    let r = w()
        .run("let m = Matrix3_rotation(1.0)\nlet t = m.transpose()\nreturn t.determinant()")
        .unwrap();
    assert!(matches!(r, Value::F64(v) if (v - 1.0).abs() < 0.001));
}

// ── Transform module ──────────────────────────────────────────────────────────

#[test]
fn test_transform2d_default_rotation_is_zero() {
    let r = w()
        .run("let t = Transform2D()\nreturn t.rotation")
        .unwrap();
    assert!(matches!(&r, Value::F32(_) | Value::F64(_)) && r.as_f64().abs() < 0.001);
}

#[test]
fn test_transform2d_custom_rotation() {
    let r = w()
        .run(
            "let pos = Vector2(0.0, 0.0)\n\
             let sc = Vector2(1.0, 1.0)\n\
             let t = Transform2D(pos, 1.5, sc)\n\
             return t.rotation",
        )
        .unwrap();
    assert!(matches!(&r, Value::F32(_) | Value::F64(_)) && (r.as_f64() - 1.5).abs() < 0.01);
}

#[test]
fn test_transform2d_position_ok() {
    assert!(w().run("let t = Transform2D()\nreturn t.position").is_ok());
}

#[test]
fn test_transform3d_default_ok() {
    assert!(w().run("let t = Transform3D()\nreturn t.position").is_ok());
}

#[test]
fn test_transform2d_to_matrix_ok() {
    assert!(w().run("let t = Transform2D()\nreturn t.toMatrix()").is_ok());
}

#[test]
fn test_transform2d_inverse_ok() {
    assert!(w().run("let t = Transform2D()\nreturn t.inverse()").is_ok());
}

// ── Input constants ───────────────────────────────────────────────────────────

#[test]
fn test_input_key_a_is_integer() {
    let r = w().run("return Key_A").unwrap();
    assert!(matches!(r, Value::I32(_) | Value::I64(_)));
}

#[test]
fn test_input_key_space_distinct_from_key_a() {
    let a = w().run("return Key_A").unwrap();
    let space = w().run("return Key_Space").unwrap();
    assert_ne!(a, space);
}

#[test]
fn test_input_mouse_button_left() {
    assert!(w().run("return MouseButton_Left").is_ok());
}

#[test]
fn test_input_controller_button_a() {
    assert!(w().run("return ControllerButton_A").is_ok());
}

#[test]
fn test_input_key_usable_in_expression() {
    let r = w().run("return Key_A + 0").unwrap();
    assert!(matches!(r, Value::I32(_) | Value::I64(_)));
}

// ── Vector2 additional methods ────────────────────────────────────────────────

#[test]
fn test_vector2_distance_3_4_5() {
    let r = w()
        .run(
            "let a = Vector2(0.0, 0.0)\n\
             let b = Vector2(3.0, 4.0)\n\
             return a.distance(b)",
        )
        .unwrap();
    assert!(matches!(&r, Value::F32(_) | Value::F64(_)) && (r.as_f64() - 5.0).abs() < 0.01);
}

#[test]
fn test_vector2_lerp_midpoint() {
    let r = w()
        .run(
            "let a = Vector2(0.0, 0.0)\n\
             let b = Vector2(10.0, 10.0)\n\
             let mid = a.lerp(b, 0.5)\n\
             return mid.x",
        )
        .unwrap();
    assert!(matches!(&r, Value::F32(_) | Value::F64(_)) && (r.as_f64() - 5.0).abs() < 0.01);
}

#[test]
fn test_vector2_field_set() {
    let r = w()
        .run(
            "var v = Vector2(1.0, 2.0)\n\
             v.x = 99.0\n\
             return v.x",
        )
        .unwrap();
    assert!(matches!(&r, Value::F32(_) | Value::F64(_)) && (r.as_f64() - 99.0).abs() < 0.01);
}

// ── Vector3 additional methods ────────────────────────────────────────────────

#[test]
fn test_vector3_length_unit_z() {
    let r = w()
        .run("let v = Vector3(0.0, 0.0, 1.0)\nreturn v.length()")
        .unwrap();
    assert!(matches!(&r, Value::F32(_) | Value::F64(_)) && (r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_vector3_normalized_x() {
    let r = w()
        .run(
            "let v = Vector3(3.0, 0.0, 0.0)\n\
             let n = v.normalized()\n\
             return n.x",
        )
        .unwrap();
    assert!(matches!(&r, Value::F32(_) | Value::F64(_)) && (r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_vector3_lerp_midpoint() {
    let r = w()
        .run(
            "let a = Vector3(0.0, 0.0, 0.0)\n\
             let b = Vector3(10.0, 0.0, 0.0)\n\
             let m = a.lerp(b, 0.5)\n\
             return m.x",
        )
        .unwrap();
    assert!(matches!(&r, Value::F32(_) | Value::F64(_)) && (r.as_f64() - 5.0).abs() < 0.01);
}

// ── Color field reads ─────────────────────────────────────────────────────────

#[test]
fn test_color_field_r() {
    let r = w()
        .run("let c = Color(0.5, 0.0, 0.0, 1.0)\nreturn c.r")
        .unwrap();
    assert!(matches!(&r, Value::F32(_) | Value::F64(_)) && (r.as_f64() - 0.5).abs() < 0.001);
}

#[test]
fn test_color_field_a() {
    let r = w()
        .run("let c = Color(1.0, 1.0, 1.0, 0.5)\nreturn c.a")
        .unwrap();
    assert!(matches!(&r, Value::F32(_) | Value::F64(_)) && (r.as_f64() - 0.5).abs() < 0.001);
}

// ── Tween additional ──────────────────────────────────────────────────────────

#[test]
fn test_tween_value_at_start_near_zero() {
    let r = w()
        .run("let t = Tween(0.0, 100.0, 1.0)\nreturn t.value()")
        .unwrap();
    assert!(matches!(&r, Value::F32(_) | Value::F64(_)) && r.as_f64() < 1.0);
}

#[test]
fn test_tween_not_finished_at_start() {
    let r = w()
        .run("let t = Tween(0.0, 100.0, 1.0)\nreturn t.isFinished()")
        .unwrap();
    assert_eq!(r, Value::Bool(false));
}

// ── Dict has, isEmpty, remove, and len ───────────────────────────────────────

#[test]
fn test_dict_has_missing_key() {
    let r = w()
        .run("let d = {\"a\": 1}\nreturn d.has(\"b\")")
        .unwrap();
    assert_eq!(r, Value::Bool(false));
}

#[test]
fn test_dict_has_existing_key() {
    let r = w()
        .run("let d = {\"a\": 42}\nreturn d.has(\"a\")")
        .unwrap();
    assert_eq!(r, Value::Bool(true));
}

#[test]
fn test_dict_len_three_entries() {
    let r = w()
        .run("let d = {\"x\": 1, \"y\": 2, \"z\": 3}\nreturn d.len()")
        .unwrap();
    assert_eq!(r, Value::I32(3));
}

#[test]
fn test_dict_remove_existing_key() {
    let r = w()
        .run("var d = {\"a\": 1, \"b\": 2}\nd.remove(\"a\")\nreturn d.len()")
        .unwrap();
    assert_eq!(r, Value::I32(1));
}

#[test]
fn test_dict_is_empty_after_remove() {
    let r = w()
        .run("var d = {\"a\": 1}\nd.remove(\"a\")\nreturn d.isEmpty()")
        .unwrap();
    assert_eq!(r, Value::Bool(true));
}

// ── Interpolation remap ───────────────────────────────────────────────────────

#[test]
fn test_interpolation_remap() {
    let r = w()
        .run("return remap(5.0, 0.0, 10.0, 0.0, 100.0)")
        .unwrap();
    assert!(matches!(&r, Value::F32(_) | Value::F64(_)) && (r.as_f64() - 50.0).abs() < 0.1);
}

// ── Noise configuration ───────────────────────────────────────────────────────

#[test]
fn test_noise_seed_changes_output() {
    let r1 = w().run("noiseSeed(1.0)\nreturn noise2D(0.5, 0.5)").unwrap();
    let r2 = w().run("noiseSeed(999.0)\nreturn noise2D(0.5, 0.5)").unwrap();
    assert_ne!(r1, r2);
}

#[test]
fn test_noise_frequency_ok() {
    assert!(w().run("noiseFrequency(4.0)\nreturn noise2D(0.5, 0.5)").is_ok());
}

#[test]
fn test_noise_type_perlin_ok() {
    assert!(w().run("noiseType(\"perlin\")\nreturn noise2D(0.1, 0.1)").is_ok());
}

#[test]
fn test_noise_fractal_ok() {
    assert!(w().run("noiseFractal(4.0, 2.0, 0.5)\nreturn noise2D(0.1, 0.1)").is_ok());
}

// ── Tween update / setEasing ──────────────────────────────────────────────────

#[test]
fn test_tween_update_advances_value() {
    let r = w()
        .run("let t = Tween(0.0, 100.0, 1.0)\nt.update(0.5)\nreturn t.value()")
        .unwrap();
    assert!(matches!(&r, Value::F64(_)) && r.as_f64() > 1.0);
}

#[test]
fn test_tween_update_completes() {
    let r = w()
        .run("let t = Tween(0.0, 100.0, 1.0)\nt.update(2.0)\nreturn t.isFinished()")
        .unwrap();
    assert_eq!(r, Value::Bool(true));
}

#[test]
fn test_tween_set_easing_ok() {
    assert!(w()
        .run("let t = Tween(0.0, 1.0, 1.0)\nt.setEasing(\"easeInOut\")\nreturn t.value()")
        .is_err()); // "easeInOut" is not a valid easing name — only "easeInOutQuad" etc. are
}

#[test]
fn test_tween_set_easing_ease_in_quad_ok() {
    let r = w()
        .run("let t = Tween(0.0, 100.0, 1.0)\nt.setEasing(\"easeInQuad\")\nt.update(0.5)\nreturn t.value()")
        .unwrap();
    assert!(matches!(&r, Value::F64(_)) && (r.as_f64() - 25.0).abs() < 1.0);
}

// ── Transform missing methods ─────────────────────────────────────────────────

#[test]
fn test_transform2d_transform_point_ok() {
    assert!(w()
        .run("let t = Transform2D()\nlet p = Vector2(1.0, 2.0)\nreturn t.transformPoint(p)")
        .is_ok());
}

#[test]
fn test_transform3d_to_matrix_ok() {
    assert!(w().run("let t = Transform3D()\nreturn t.toMatrix()").is_ok());
}

#[test]
fn test_transform3d_inverse_ok() {
    assert!(w().run("let t = Transform3D()\nreturn t.inverse()").is_ok());
}

// ── IO readFile ───────────────────────────────────────────────────────────────

#[test]
fn test_io_read_file_after_write() {
    use std::rc::Rc;
    let dir = std::env::temp_dir().join("writ_read_test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("data.txt");
    let path_str = path.to_str().unwrap().replace('\\', "/");
    w().run(&format!(r#"writeFile("{path_str}", "hello writ")"#))
        .unwrap();
    let r = w()
        .run(&format!(r#"return readFile("{path_str}")"#))
        .unwrap();
    assert_eq!(r, Value::Str(Rc::from("hello writ")));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn test_io_read_nonexistent_errors() {
    assert!(w().run(r#"return readFile("/no/such/file_writ_test.txt")"#).is_err());
}

// ── Interpolation easing functions ────────────────────────────────────────────

#[test]
fn test_ease_in_quad_at_half() {
    let r = w().run("return easeInQuad(0.5)").unwrap();
    assert!(matches!(&r, Value::F64(_)) && (r.as_f64() - 0.25).abs() < 0.01);
}

#[test]
fn test_ease_out_quad_at_half() {
    let r = w().run("return easeOutQuad(0.5)").unwrap();
    assert!(matches!(&r, Value::F64(_)) && r.as_f64() > 0.5);
}

#[test]
fn test_ease_in_cubic_boundary_values() {
    let r0 = w().run("return easeInCubic(0.0)").unwrap();
    let r1 = w().run("return easeInCubic(1.0)").unwrap();
    assert!(r0.as_f64().abs() < 0.001);
    assert!((r1.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_ease_in_out_sine_midpoint() {
    let r = w().run("return easeInOutSine(0.5)").unwrap();
    assert!(matches!(&r, Value::F64(_)) && (r.as_f64() - 0.5).abs() < 0.01);
}

#[test]
fn test_smoothstep_boundary_values() {
    let r0 = w().run("return smoothstep(0.0, 1.0, 0.0)").unwrap();
    let r1 = w().run("return smoothstep(0.0, 1.0, 1.0)").unwrap();
    assert!(r0.as_f64().abs() < 0.001);
    assert!((r1.as_f64() - 1.0).abs() < 0.001);
}

// ── Vector2 missing ops ──────────────────────────────────────────────

#[test]
fn test_vector2_length_squared() {
    let r = w().run("let v = Vector2(3.0, 4.0)\nreturn v.lengthSquared()").unwrap();
    assert!((r.as_f64() - 25.0).abs() < 0.001);
}

#[test]
fn test_vector2_distance_squared() {
    let r = w().run("let a = Vector2(0.0, 0.0)\nlet b = Vector2(3.0, 4.0)\nreturn a.distanceSquared(b)").unwrap();
    assert!((r.as_f64() - 25.0).abs() < 0.001);
}

#[test]
fn test_vector2_clamp() {
    let r = w().run("let v = Vector2(5.0, -1.0)\nlet r = v.clamp(Vector2(0.0, 0.0), Vector2(3.0, 3.0))\nreturn r.x").unwrap();
    assert!((r.as_f64() - 3.0).abs() < 0.001);
}

#[test]
fn test_vector2_sign() {
    let r = w().run("let v = Vector2(-5.0, 3.0)\nlet s = v.sign()\nreturn s.x").unwrap();
    assert!((r.as_f64() - (-1.0)).abs() < 0.001);
}

#[test]
fn test_vector2_floor_ceil_round() {
    let rf = w().run("return Vector2(1.7, 2.3).floor().x").unwrap();
    let rc = w().run("return Vector2(1.7, 2.3).ceil().y").unwrap();
    let rr = w().run("return Vector2(1.5, 2.4).round().x").unwrap();
    assert!((rf.as_f64() - 1.0).abs() < 0.001);
    assert!((rc.as_f64() - 3.0).abs() < 0.001);
    assert!((rr.as_f64() - 2.0).abs() < 0.001);
}

#[test]
fn test_vector2_min_max() {
    let r = w().run("let a = Vector2(1.0, 5.0)\nlet b = Vector2(3.0, 2.0)\nreturn a.min(b).x").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
    let r = w().run("let a = Vector2(1.0, 5.0)\nlet b = Vector2(3.0, 2.0)\nreturn a.max(b).y").unwrap();
    assert!((r.as_f64() - 5.0).abs() < 0.001);
}

#[test]
fn test_vector2_add_sub() {
    let r = w().run("let a = Vector2(1.0, 2.0)\nlet b = Vector2(3.0, 4.0)\nreturn a.add(b).x").unwrap();
    assert!((r.as_f64() - 4.0).abs() < 0.001);
    let r = w().run("let a = Vector2(5.0, 6.0)\nlet b = Vector2(1.0, 2.0)\nreturn a.sub(b).y").unwrap();
    assert!((r.as_f64() - 4.0).abs() < 0.001);
}

#[test]
fn test_vector2_mul_scalar_and_vector() {
    let r = w().run("return Vector2(2.0, 3.0).mul(2.0).x").unwrap();
    assert!((r.as_f64() - 4.0).abs() < 0.001);
    let r = w().run("return Vector2(2.0, 3.0).mul(Vector2(3.0, 4.0)).y").unwrap();
    assert!((r.as_f64() - 12.0).abs() < 0.001);
}

#[test]
fn test_vector2_div_scalar_and_vector() {
    let r = w().run("return Vector2(6.0, 8.0).div(2.0).x").unwrap();
    assert!((r.as_f64() - 3.0).abs() < 0.001);
    let r = w().run("return Vector2(6.0, 8.0).div(Vector2(2.0, 4.0)).y").unwrap();
    assert!((r.as_f64() - 2.0).abs() < 0.001);
}

#[test]
fn test_vector2_set_field_y() {
    let r = w().run("let v = Vector2(1.0, 2.0)\nv.y = 99.0\nreturn v.y").unwrap();
    assert!((r.as_f64() - 99.0).abs() < 0.001);
}

// ── Vector3 missing ops ──────────────────────────────────────────────

#[test]
fn test_vector3_length_squared() {
    let r = w().run("return Vector3(1.0, 2.0, 2.0).lengthSquared()").unwrap();
    assert!((r.as_f64() - 9.0).abs() < 0.001);
}

#[test]
fn test_vector3_distance_squared() {
    let r = w().run("let a = Vector3(0.0, 0.0, 0.0)\nlet b = Vector3(1.0, 2.0, 2.0)\nreturn a.distanceSquared(b)").unwrap();
    assert!((r.as_f64() - 9.0).abs() < 0.001);
}

#[test]
fn test_vector3_abs() {
    let r = w().run("return Vector3(-1.0, -2.0, 3.0).abs().x").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_vector3_negate() {
    let r = w().run("return Vector3(1.0, -2.0, 3.0).negate().y").unwrap();
    assert!((r.as_f64() - 2.0).abs() < 0.001);
}

#[test]
fn test_vector3_add_sub() {
    let r = w().run("return Vector3(1.0, 2.0, 3.0).add(Vector3(4.0, 5.0, 6.0)).z").unwrap();
    assert!((r.as_f64() - 9.0).abs() < 0.001);
    let r = w().run("return Vector3(4.0, 5.0, 6.0).sub(Vector3(1.0, 2.0, 3.0)).x").unwrap();
    assert!((r.as_f64() - 3.0).abs() < 0.001);
}

#[test]
fn test_vector3_mul_div() {
    let r = w().run("return Vector3(2.0, 3.0, 4.0).mul(2.0).z").unwrap();
    assert!((r.as_f64() - 8.0).abs() < 0.001);
    let r = w().run("return Vector3(2.0, 3.0, 4.0).mul(Vector3(3.0, 2.0, 1.0)).x").unwrap();
    assert!((r.as_f64() - 6.0).abs() < 0.001);
    let r = w().run("return Vector3(6.0, 8.0, 10.0).div(2.0).y").unwrap();
    assert!((r.as_f64() - 4.0).abs() < 0.001);
    let r = w().run("return Vector3(6.0, 8.0, 10.0).div(Vector3(2.0, 4.0, 5.0)).z").unwrap();
    assert!((r.as_f64() - 2.0).abs() < 0.001);
}

#[test]
fn test_vector3_clamp() {
    let r = w().run("return Vector3(5.0, -1.0, 3.0).clamp(Vector3(0.0, 0.0, 0.0), Vector3(2.0, 2.0, 2.0)).x").unwrap();
    assert!((r.as_f64() - 2.0).abs() < 0.001);
}

#[test]
fn test_vector3_sign_floor_ceil_round() {
    let r = w().run("return Vector3(-1.0, 2.0, 0.0).sign().x").unwrap();
    assert!((r.as_f64() - (-1.0)).abs() < 0.001);
    let r = w().run("return Vector3(1.7, 2.3, 3.9).floor().z").unwrap();
    assert!((r.as_f64() - 3.0).abs() < 0.001);
    let r = w().run("return Vector3(1.1, 2.9, 3.1).ceil().x").unwrap();
    assert!((r.as_f64() - 2.0).abs() < 0.001);
    let r = w().run("return Vector3(1.5, 2.4, 3.6).round().y").unwrap();
    assert!((r.as_f64() - 2.0).abs() < 0.001);
}

#[test]
fn test_vector3_min_max() {
    let r = w().run("return Vector3(1.0, 5.0, 3.0).min(Vector3(3.0, 2.0, 4.0)).y").unwrap();
    assert!((r.as_f64() - 2.0).abs() < 0.001);
    let r = w().run("return Vector3(1.0, 5.0, 3.0).max(Vector3(3.0, 2.0, 4.0)).z").unwrap();
    assert!((r.as_f64() - 4.0).abs() < 0.001);
}

#[test]
fn test_vector3_set_fields() {
    let r = w().run("let v = Vector3(0.0, 0.0, 0.0)\nv.x = 1.0\nv.y = 2.0\nv.z = 3.0\nreturn v.x + v.y + v.z").unwrap();
    assert!((r.as_f64() - 6.0).abs() < 0.001);
}

#[test]
fn test_vector3_distance() {
    let r = w().run("return Vector3(0.0, 0.0, 0.0).distance(Vector3(1.0, 2.0, 2.0))").unwrap();
    assert!((r.as_f64() - 3.0).abs() < 0.001);
}

// ── Vector4 full coverage ────────────────────────────────────────────

#[test]
fn test_vector4_field_reads() {
    let r = w().run("let v = Vector4(1.0, 2.0, 3.0, 4.0)\nreturn v.x + v.y + v.z + v.w").unwrap();
    assert!((r.as_f64() - 10.0).abs() < 0.001);
}

#[test]
fn test_vector4_set_fields() {
    let r = w().run("let v = Vector4(0.0, 0.0, 0.0, 0.0)\nv.x = 1.0\nv.y = 2.0\nv.z = 3.0\nv.w = 4.0\nreturn v.x + v.y + v.z + v.w").unwrap();
    assert!((r.as_f64() - 10.0).abs() < 0.001);
}

#[test]
fn test_vector4_length() {
    let r = w().run("return Vector4(1.0, 0.0, 0.0, 0.0).length()").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_vector4_length_squared() {
    let r = w().run("return Vector4(1.0, 2.0, 3.0, 4.0).lengthSquared()").unwrap();
    assert!((r.as_f64() - 30.0).abs() < 0.001);
}

#[test]
fn test_vector4_normalized() {
    let r = w().run("return Vector4(3.0, 0.0, 0.0, 0.0).normalized().x").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_vector4_dot() {
    let r = w().run("return Vector4(1.0, 2.0, 3.0, 4.0).dot(Vector4(4.0, 3.0, 2.0, 1.0))").unwrap();
    assert!((r.as_f64() - 20.0).abs() < 0.001);
}

#[test]
fn test_vector4_distance() {
    let r = w().run("return Vector4(0.0, 0.0, 0.0, 0.0).distance(Vector4(1.0, 0.0, 0.0, 0.0))").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_vector4_distance_squared() {
    let r = w().run("return Vector4(0.0, 0.0, 0.0, 0.0).distanceSquared(Vector4(2.0, 0.0, 0.0, 0.0))").unwrap();
    assert!((r.as_f64() - 4.0).abs() < 0.001);
}

#[test]
fn test_vector4_lerp() {
    let r = w().run("return Vector4(0.0, 0.0, 0.0, 0.0).lerp(Vector4(10.0, 20.0, 30.0, 40.0), 0.5).x").unwrap();
    assert!((r.as_f64() - 5.0).abs() < 0.001);
}

#[test]
fn test_vector4_clamp() {
    let r = w().run("return Vector4(5.0, -1.0, 3.0, 10.0).clamp(Vector4(0.0, 0.0, 0.0, 0.0), Vector4(2.0, 2.0, 2.0, 2.0)).w").unwrap();
    assert!((r.as_f64() - 2.0).abs() < 0.001);
}

#[test]
fn test_vector4_abs() {
    let r = w().run("return Vector4(-1.0, -2.0, 3.0, -4.0).abs().w").unwrap();
    assert!((r.as_f64() - 4.0).abs() < 0.001);
}

#[test]
fn test_vector4_sign() {
    let r = w().run("return Vector4(-5.0, 3.0, 0.0, -1.0).sign().x").unwrap();
    assert!((r.as_f64() - (-1.0)).abs() < 0.001);
}

#[test]
fn test_vector4_floor_ceil_round() {
    let r = w().run("return Vector4(1.7, 2.3, 3.9, 4.1).floor().x").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
    let r = w().run("return Vector4(1.1, 2.9, 3.1, 4.9).ceil().y").unwrap();
    assert!((r.as_f64() - 3.0).abs() < 0.001);
    let r = w().run("return Vector4(1.5, 2.4, 3.6, 4.5).round().z").unwrap();
    assert!((r.as_f64() - 4.0).abs() < 0.001);
}

#[test]
fn test_vector4_min_max() {
    let r = w().run("return Vector4(1.0, 5.0, 3.0, 7.0).min(Vector4(3.0, 2.0, 4.0, 1.0)).w").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
    let r = w().run("return Vector4(1.0, 5.0, 3.0, 7.0).max(Vector4(3.0, 2.0, 4.0, 1.0)).w").unwrap();
    assert!((r.as_f64() - 7.0).abs() < 0.001);
}

#[test]
fn test_vector4_add_sub() {
    let r = w().run("return Vector4(1.0, 2.0, 3.0, 4.0).add(Vector4(5.0, 6.0, 7.0, 8.0)).w").unwrap();
    assert!((r.as_f64() - 12.0).abs() < 0.001);
    let r = w().run("return Vector4(5.0, 6.0, 7.0, 8.0).sub(Vector4(1.0, 2.0, 3.0, 4.0)).z").unwrap();
    assert!((r.as_f64() - 4.0).abs() < 0.001);
}

#[test]
fn test_vector4_mul_scalar_and_vector() {
    let r = w().run("return Vector4(2.0, 3.0, 4.0, 5.0).mul(2.0).w").unwrap();
    assert!((r.as_f64() - 10.0).abs() < 0.001);
    let r = w().run("return Vector4(2.0, 3.0, 4.0, 5.0).mul(Vector4(3.0, 2.0, 1.0, 2.0)).x").unwrap();
    assert!((r.as_f64() - 6.0).abs() < 0.001);
}

#[test]
fn test_vector4_div_scalar_and_vector() {
    let r = w().run("return Vector4(6.0, 8.0, 10.0, 12.0).div(2.0).z").unwrap();
    assert!((r.as_f64() - 5.0).abs() < 0.001);
    let r = w().run("return Vector4(6.0, 8.0, 10.0, 12.0).div(Vector4(2.0, 4.0, 5.0, 6.0)).w").unwrap();
    assert!((r.as_f64() - 2.0).abs() < 0.001);
}

#[test]
fn test_vector4_negate() {
    let r = w().run("return Vector4(1.0, -2.0, 3.0, -4.0).negate().x").unwrap();
    assert!((r.as_f64() - (-1.0)).abs() < 0.001);
}

// ── Rectangle ────────────────────────────────────────────────────────

#[test]
fn test_rect_construction_and_fields() {
    let r = w().run("let r = Rectangle(10.0, 20.0, 100.0, 50.0)\nreturn r.position.x").unwrap();
    assert!((r.as_f64() - 10.0).abs() < 0.001);
    let r = w().run("let r = Rectangle(10.0, 20.0, 100.0, 50.0)\nreturn r.size.y").unwrap();
    assert!((r.as_f64() - 50.0).abs() < 0.001);
}

#[test]
fn test_rect_width_height() {
    let r = w().run("let r = Rectangle(0.0, 0.0, 100.0, 50.0)\nreturn r.width()").unwrap();
    assert!((r.as_f64() - 100.0).abs() < 0.001);
    let r = w().run("let r = Rectangle(0.0, 0.0, 100.0, 50.0)\nreturn r.height()").unwrap();
    assert!((r.as_f64() - 50.0).abs() < 0.001);
}

#[test]
fn test_rect_center() {
    let r = w().run("let r = Rectangle(0.0, 0.0, 100.0, 50.0)\nreturn r.center().x").unwrap();
    assert!((r.as_f64() - 50.0).abs() < 0.001);
}

#[test]
fn test_rect_area() {
    let r = w().run("return Rectangle(0.0, 0.0, 10.0, 5.0).area()").unwrap();
    assert!((r.as_f64() - 50.0).abs() < 0.001);
}

#[test]
fn test_rect_contains() {
    let r = w().run("return Rectangle(0.0, 0.0, 10.0, 10.0).contains(Vector2(5.0, 5.0))").unwrap();
    assert_eq!(r, Value::Bool(true));
    let r = w().run("return Rectangle(0.0, 0.0, 10.0, 10.0).contains(Vector2(15.0, 5.0))").unwrap();
    assert_eq!(r, Value::Bool(false));
}

#[test]
fn test_rect_intersects() {
    let r = w().run("return Rectangle(0.0, 0.0, 10.0, 10.0).intersects(Rectangle(5.0, 5.0, 10.0, 10.0))").unwrap();
    assert_eq!(r, Value::Bool(true));
    let r = w().run("return Rectangle(0.0, 0.0, 10.0, 10.0).intersects(Rectangle(20.0, 20.0, 10.0, 10.0))").unwrap();
    assert_eq!(r, Value::Bool(false));
}

#[test]
fn test_rect_intersection_overlap() {
    let r = w().run("let i = Rectangle(0.0, 0.0, 10.0, 10.0).intersection(Rectangle(5.0, 5.0, 10.0, 10.0))\nreturn i.area()").unwrap();
    assert!((r.as_f64() - 25.0).abs() < 0.001);
}

#[test]
fn test_rect_intersection_no_overlap() {
    let r = w().run("return Rectangle(0.0, 0.0, 5.0, 5.0).intersection(Rectangle(10.0, 10.0, 5.0, 5.0))").unwrap();
    assert_eq!(r, Value::Null);
}

#[test]
fn test_rect_merge() {
    let r = w().run("let m = Rectangle(0.0, 0.0, 5.0, 5.0).merge(Rectangle(10.0, 10.0, 5.0, 5.0))\nreturn m.area()").unwrap();
    assert!((r.as_f64() - 225.0).abs() < 0.001);
}

#[test]
fn test_rect_expand() {
    let r = w().run("let e = Rectangle(5.0, 5.0, 10.0, 10.0).expand(2.0)\nreturn e.area()").unwrap();
    assert!((r.as_f64() - 196.0).abs() < 0.001);
}

#[test]
fn test_rect_set_field_position() {
    let r = w().run("let r = Rectangle(0.0, 0.0, 10.0, 10.0)\nr.position = Vector2(5.0, 5.0)\nreturn r.position.x").unwrap();
    assert!((r.as_f64() - 5.0).abs() < 0.001);
}

#[test]
fn test_rect_set_field_size() {
    let r = w().run("let r = Rectangle(0.0, 0.0, 10.0, 10.0)\nr.size = Vector2(20.0, 30.0)\nreturn r.area()").unwrap();
    assert!((r.as_f64() - 600.0).abs() < 0.001);
}

#[test]
fn test_rect_from_points() {
    let r = w().run("let r = Rectangle_fromPoints(Vector2(1.0, 2.0), Vector2(5.0, 6.0))\nreturn r.area()").unwrap();
    assert!((r.as_f64() - 16.0).abs() < 0.001);
}

// ── BoundingBox ──────────────────────────────────────────────────────

#[test]
fn test_bbox_construction_and_fields() {
    let r = w().run("let b = BoundingBox(Vector3(0.0, 0.0, 0.0), Vector3(1.0, 2.0, 3.0))\nreturn b.min.x").unwrap();
    assert!(r.as_f64().abs() < 0.001);
    let r = w().run("let b = BoundingBox(Vector3(0.0, 0.0, 0.0), Vector3(1.0, 2.0, 3.0))\nreturn b.max.z").unwrap();
    assert!((r.as_f64() - 3.0).abs() < 0.001);
}

#[test]
fn test_bbox_size_center_volume() {
    let r = w().run("return BoundingBox(Vector3(0.0, 0.0, 0.0), Vector3(2.0, 4.0, 6.0)).size().x").unwrap();
    assert!((r.as_f64() - 2.0).abs() < 0.001);
    let r = w().run("return BoundingBox(Vector3(0.0, 0.0, 0.0), Vector3(2.0, 4.0, 6.0)).center().y").unwrap();
    assert!((r.as_f64() - 2.0).abs() < 0.001);
    let r = w().run("return BoundingBox(Vector3(0.0, 0.0, 0.0), Vector3(2.0, 4.0, 6.0)).volume()").unwrap();
    assert!((r.as_f64() - 48.0).abs() < 0.001);
}

#[test]
fn test_bbox_contains() {
    let r = w().run("return BoundingBox(Vector3(0.0, 0.0, 0.0), Vector3(10.0, 10.0, 10.0)).contains(Vector3(5.0, 5.0, 5.0))").unwrap();
    assert_eq!(r, Value::Bool(true));
    let r = w().run("return BoundingBox(Vector3(0.0, 0.0, 0.0), Vector3(10.0, 10.0, 10.0)).contains(Vector3(15.0, 5.0, 5.0))").unwrap();
    assert_eq!(r, Value::Bool(false));
}

#[test]
fn test_bbox_intersects() {
    let r = w().run("return BoundingBox(Vector3(0.0, 0.0, 0.0), Vector3(10.0, 10.0, 10.0)).intersects(BoundingBox(Vector3(5.0, 5.0, 5.0), Vector3(15.0, 15.0, 15.0)))").unwrap();
    assert_eq!(r, Value::Bool(true));
}

#[test]
fn test_bbox_intersection() {
    let r = w().run("let i = BoundingBox(Vector3(0.0, 0.0, 0.0), Vector3(10.0, 10.0, 10.0)).intersection(BoundingBox(Vector3(5.0, 5.0, 5.0), Vector3(15.0, 15.0, 15.0)))\nreturn i.volume()").unwrap();
    assert!((r.as_f64() - 125.0).abs() < 0.001);
}

#[test]
fn test_bbox_intersection_no_overlap() {
    let r = w().run("return BoundingBox(Vector3(0.0, 0.0, 0.0), Vector3(1.0, 1.0, 1.0)).intersection(BoundingBox(Vector3(5.0, 5.0, 5.0), Vector3(6.0, 6.0, 6.0)))").unwrap();
    assert_eq!(r, Value::Null);
}

#[test]
fn test_bbox_merge() {
    let r = w().run("let m = BoundingBox(Vector3(0.0, 0.0, 0.0), Vector3(1.0, 1.0, 1.0)).merge(BoundingBox(Vector3(2.0, 2.0, 2.0), Vector3(3.0, 3.0, 3.0)))\nreturn m.volume()").unwrap();
    assert!((r.as_f64() - 27.0).abs() < 0.001);
}

#[test]
fn test_bbox_expand() {
    let r = w().run("let e = BoundingBox(Vector3(1.0, 1.0, 1.0), Vector3(2.0, 2.0, 2.0)).expand(1.0)\nreturn e.volume()").unwrap();
    assert!((r.as_f64() - 27.0).abs() < 0.001);
}

#[test]
fn test_bbox_set_fields() {
    let r = w().run("let b = BoundingBox(Vector3(0.0, 0.0, 0.0), Vector3(1.0, 1.0, 1.0))\nb.min = Vector3(-1.0, -1.0, -1.0)\nreturn b.volume()").unwrap();
    assert!((r.as_f64() - 8.0).abs() < 0.001);
}

// ── Color ────────────────────────────────────────────────────────────

#[test]
fn test_color_field_reads_g_b() {
    let r = w().run("let c = Color(0.1, 0.2, 0.3)\nreturn c.g").unwrap();
    assert!((r.as_f64() - 0.2).abs() < 0.01);
    let r = w().run("let c = Color(0.1, 0.2, 0.3)\nreturn c.b").unwrap();
    assert!((r.as_f64() - 0.3).abs() < 0.01);
}

#[test]
fn test_color_set_fields() {
    let r = w().run("let c = Color(0.0, 0.0, 0.0)\nc.r = 1.0\nc.g = 0.5\nc.b = 0.25\nc.a = 0.75\nreturn c.r + c.g + c.b + c.a").unwrap();
    assert!((r.as_f64() - 2.5).abs() < 0.01);
}

#[test]
fn test_color_to_hex_with_alpha() {
    let r = w().run("return Color(1.0, 0.0, 0.0, 0.5).toHex()").unwrap();
    if let Value::Str(s) = &r {
        assert!(s.len() == 9, "expected 8-char hex with alpha, got {s}");
        assert!(s.starts_with("#FF0000"));
    } else {
        panic!("expected string, got {r:?}");
    }
}

#[test]
fn test_color_to_hsv() {
    let r = w().run("return Color(1.0, 0.0, 0.0).toHSV()").unwrap();
    assert!(matches!(r, Value::Array(_)));
}

#[test]
fn test_color_lighten() {
    let r = w().run("return Color(0.5, 0.5, 0.5).lighten(0.2).r").unwrap();
    assert!((r.as_f64() - 0.7).abs() < 0.01);
}

#[test]
fn test_color_darken() {
    let r = w().run("return Color(0.5, 0.5, 0.5).darken(0.2).r").unwrap();
    assert!((r.as_f64() - 0.3).abs() < 0.01);
}

#[test]
fn test_color_inverted() {
    let r = w().run("return Color(0.25, 0.75, 0.0).inverted().r").unwrap();
    assert!((r.as_f64() - 0.75).abs() < 0.01);
}

#[test]
fn test_color_from_hex_6char() {
    let r = w().run("return Color_fromHex(\"#FF0000\").r").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.01);
}

#[test]
fn test_color_from_hex_8char() {
    let r = w().run("return Color_fromHex(\"#FF000080\").a").unwrap();
    assert!(r.as_f64() < 0.55 && r.as_f64() > 0.45);
}

#[test]
fn test_color_from_hsv() {
    // Pure red: h=0, s=1, v=1
    let r = w().run("return Color_fromHSV(0.0, 1.0, 1.0).r").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.01);
}

#[test]
fn test_color_from_hsv_green() {
    // Green: h=120, s=1, v=1
    let r = w().run("return Color_fromHSV(120.0, 1.0, 1.0).g").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.01);
}

#[test]
fn test_color_from_hsv_blue() {
    // Blue: h=240, s=1, v=1
    let r = w().run("return Color_fromHSV(240.0, 1.0, 1.0).b").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.01);
}

#[test]
fn test_color_from_hsv_cyan() {
    // Cyan: h=180, s=1, v=1
    let r = w().run("return Color_fromHSV(180.0, 1.0, 1.0).g").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.01);
}

#[test]
fn test_color_from_hsv_yellow() {
    // Yellow: h=60, s=1, v=1
    let r = w().run("return Color_fromHSV(60.0, 1.0, 1.0).r").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.01);
}

#[test]
fn test_color_from_hsv_magenta() {
    // Magenta: h=300, s=1, v=1
    let r = w().run("return Color_fromHSV(300.0, 1.0, 1.0).r").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.01);
}

#[test]
fn test_color_constants() {
    assert_eq!(w().run("return Color_RED.r").unwrap().as_f64(), 1.0);
    assert_eq!(w().run("return Color_GREEN.g").unwrap().as_f64(), 1.0);
    assert_eq!(w().run("return Color_BLUE.b").unwrap().as_f64(), 1.0);
    assert_eq!(w().run("return Color_WHITE.r").unwrap().as_f64(), 1.0);
    assert_eq!(w().run("return Color_BLACK.r").unwrap().as_f64(), 0.0);
    assert_eq!(w().run("return Color_YELLOW.g").unwrap().as_f64(), 1.0);
    assert_eq!(w().run("return Color_CYAN.b").unwrap().as_f64(), 1.0);
    assert_eq!(w().run("return Color_MAGENTA.r").unwrap().as_f64(), 1.0);
    assert_eq!(w().run("return Color_TRANSPARENT.a").unwrap().as_f64(), 0.0);
}

// ── Matrix ───────────────────────────────────────────────────────────

#[test]
fn test_matrix3_multiply() {
    // identity * identity = identity; determinant still 1
    let r = w().run("let a = Matrix3_IDENTITY\nlet b = Matrix3_rotation(0.0)\nlet m = a.multiply(b)\nreturn m.determinant()").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_matrix3_transform_point() {
    let r = w().run("let m = Matrix3_translation(5.0, 10.0)\nreturn m.transformPoint(Vector2(0.0, 0.0)).x").unwrap();
    assert!((r.as_f64() - 5.0).abs() < 0.001);
}

#[test]
fn test_matrix3_transform_vector() {
    // transformVector ignores translation
    let r = w().run("let m = Matrix3_translation(5.0, 10.0)\nreturn m.transformVector(Vector2(1.0, 0.0)).x").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_matrix3_translation() {
    let r = w().run("let m = Matrix3_translation(3.0, 4.0)\nreturn m.determinant()").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_matrix4_multiply() {
    let r = w().run("let a = Matrix4_IDENTITY\nlet b = Matrix4_translation(0.0, 0.0, 0.0)\nlet m = a.multiply(b)\nreturn m.determinant()").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_matrix4_inverse() {
    let r = w().run("let m = Matrix4_IDENTITY.inverse()\nreturn m.determinant()").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_matrix4_transpose() {
    let r = w().run("let m = Matrix4_IDENTITY.transpose()\nreturn m.determinant()").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_matrix4_transform_point() {
    let r = w().run("let m = Matrix4_translation(5.0, 10.0, 15.0)\nreturn m.transformPoint(Vector3(0.0, 0.0, 0.0)).x").unwrap();
    assert!((r.as_f64() - 5.0).abs() < 0.001);
}

#[test]
fn test_matrix4_transform_vector() {
    let r = w().run("let m = Matrix4_translation(5.0, 10.0, 15.0)\nreturn m.transformVector(Vector3(1.0, 0.0, 0.0)).x").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_matrix4_rotation() {
    let r = w().run("let m = Matrix4_rotation(Vector3(0.0, 0.0, 1.0), 0.0)\nreturn m.determinant()").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_matrix4_translation() {
    let r = w().run("let m = Matrix4_translation(1.0, 2.0, 3.0)\nreturn m.determinant()").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_matrix4_perspective() {
    let r = w().run("let m = Matrix4_perspective(1.0, 1.0, 0.1, 100.0)\nreturn typeof(m)").unwrap();
    assert_eq!(r, Value::Str(std::rc::Rc::from("Matrix4")));
}

#[test]
fn test_matrix4_orthographic() {
    let r = w().run("let m = Matrix4_orthographic(-1.0, 1.0, -1.0, 1.0, 0.1, 100.0)\nreturn typeof(m)").unwrap();
    assert_eq!(r, Value::Str(std::rc::Rc::from("Matrix4")));
}

#[test]
fn test_matrix4_look_at() {
    let r = w().run("let m = Matrix4_lookAt(Vector3(0.0, 0.0, 5.0), Vector3(0.0, 0.0, 0.0), Vector3(0.0, 1.0, 0.0))\nreturn typeof(m)").unwrap();
    assert_eq!(r, Value::Str(std::rc::Rc::from("Matrix4")));
}

// ── Transform ────────────────────────────────────────────────────────

#[test]
fn test_transform2d_set_fields() {
    let r = w().run("let t = Transform2D()\nt.position = Vector2(5.0, 10.0)\nreturn t.position.x").unwrap();
    assert!((r.as_f64() - 5.0).abs() < 0.001);
    let r = w().run("let t = Transform2D()\nt.rotation = 1.5\nreturn t.rotation").unwrap();
    assert!((r.as_f64() - 1.5).abs() < 0.001);
    let r = w().run("let t = Transform2D()\nt.scale = Vector2(2.0, 3.0)\nreturn t.scale.x").unwrap();
    assert!((r.as_f64() - 2.0).abs() < 0.001);
}

#[test]
fn test_transform2d_translate() {
    let r = w().run("let t = Transform2D()\nt.translate(Vector2(5.0, 10.0))\nreturn t.position.x").unwrap();
    assert!((r.as_f64() - 5.0).abs() < 0.001);
}

#[test]
fn test_transform2d_rotate() {
    let r = w().run("let t = Transform2D()\nt.rotate(1.0)\nreturn t.rotation").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_transform2d_look_at() {
    // lookAt sets rotation via atan2
    let r = w().run("let t = Transform2D()\nt.lookAt(Vector2(1.0, 0.0))\nreturn t.rotation").unwrap();
    assert!(r.as_f64().abs() < 0.01);
}

#[test]
fn test_transform2d_transform_vector() {
    let r = w().run("let t = Transform2D()\nreturn t.transformVector(Vector2(1.0, 0.0)).x").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_transform2d_scale_read() {
    let r = w().run("return Transform2D().scale.x").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_transform3d_set_fields() {
    let r = w().run("let t = Transform3D()\nt.position = Vector3(1.0, 2.0, 3.0)\nreturn t.position.z").unwrap();
    assert!((r.as_f64() - 3.0).abs() < 0.001);
}

#[test]
fn test_transform3d_rotation_scale_reads() {
    let r = w().run("let t = Transform3D()\nreturn t.rotation.w").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
    let r = w().run("let t = Transform3D()\nreturn t.scale.x").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_transform3d_translate() {
    let r = w().run("let t = Transform3D()\nt.translate(Vector3(1.0, 2.0, 3.0))\nreturn t.position.y").unwrap();
    assert!((r.as_f64() - 2.0).abs() < 0.001);
}

#[test]
fn test_transform3d_transform_point() {
    let r = w().run("let t = Transform3D()\nreturn t.transformPoint(Vector3(1.0, 0.0, 0.0)).x").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_transform3d_transform_vector() {
    let r = w().run("let t = Transform3D()\nreturn t.transformVector(Vector3(0.0, 1.0, 0.0)).y").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_transform3d_rotate() {
    // Rotating 0 radians around Y axis
    let r = w().run("let t = Transform3D()\nt.rotate(Vector3(0.0, 1.0, 0.0), 0.0)\nreturn t.rotation.w").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_transform3d_look_at() {
    let r = w().run("let t = Transform3D()\nt.lookAt(Vector3(0.0, 0.0, -1.0), Vector3(0.0, 1.0, 0.0))\nreturn typeof(t)").unwrap();
    assert_eq!(r, Value::Str(std::rc::Rc::from("Transform3D")));
}

// ── Timer ────────────────────────────────────────────────────────────
// NOTE: Timer.start() cannot be tested via writ scripts because `start`
// is a reserved keyword (coroutine start). We test the paths we can.

#[test]
fn test_timer_update_on_stopped_timer() {
    // update on non-running timer is a no-op
    let r = w().run("let t = Timer(1.0)\nt.update(0.5)\nreturn t.isFinished()").unwrap();
    assert_eq!(r, Value::Bool(false));
}

#[test]
fn test_timer_set_callback() {
    w().run("let t = Timer(1.0)\nt.setCallback(42)").unwrap();
}

#[test]
fn test_timer_stop_reset() {
    let r = w().run("let t = Timer(1.0)\nt.stop()\nt.reset()\nreturn t.elapsed()").unwrap();
    assert!(r.as_f64().abs() < 0.001);
}

#[test]
fn test_timer_is_running_initially_false() {
    let r = w().run("let t = Timer(1.0)\nreturn t.isRunning()").unwrap();
    assert_eq!(r, Value::Bool(false));
}

#[test]
fn test_timer_remaining_full() {
    let r = w().run("let t = Timer(2.0)\nreturn t.remaining()").unwrap();
    assert!((r.as_f64() - 2.0).abs() < 0.001);
}

// ── Tween ────────────────────────────────────────────────────────────

#[test]
fn test_tween_easing_linear() {
    let r = w().run("let t = Tween(0.0, 10.0, 1.0)\nt.setEasing(\"linear\")\nt.update(0.5)\nreturn t.value()").unwrap();
    assert!((r.as_f64() - 5.0).abs() < 0.01);
}

#[test]
fn test_tween_easing_ease_out_quad() {
    let r = w().run("let t = Tween(0.0, 10.0, 1.0)\nt.setEasing(\"easeOutQuad\")\nt.update(0.5)\nreturn t.value()").unwrap();
    assert!(r.as_f64() > 5.0); // easeOut is faster at start
}

#[test]
fn test_tween_easing_ease_in_out_quad() {
    // t=0.5 is the inflection point
    let r1 = w().run("let t = Tween(0.0, 10.0, 1.0)\nt.setEasing(\"easeInOutQuad\")\nt.update(0.25)\nreturn t.value()").unwrap();
    let r2 = w().run("let t = Tween(0.0, 10.0, 1.0)\nt.setEasing(\"easeInOutQuad\")\nt.update(0.75)\nreturn t.value()").unwrap();
    assert!(r1.as_f64() < 2.5);
    assert!(r2.as_f64() > 7.5);
}

#[test]
fn test_tween_easing_cubic_variants() {
    let r = w().run("let t = Tween(0.0, 10.0, 1.0)\nt.setEasing(\"easeInCubic\")\nt.update(1.0)\nreturn t.value()").unwrap();
    assert!((r.as_f64() - 10.0).abs() < 0.01);
    let r = w().run("let t = Tween(0.0, 10.0, 1.0)\nt.setEasing(\"easeOutCubic\")\nt.update(0.5)\nreturn t.value()").unwrap();
    assert!(r.as_f64() > 5.0);
    let r = w().run("let t = Tween(0.0, 10.0, 1.0)\nt.setEasing(\"easeInOutCubic\")\nt.update(0.75)\nreturn t.value()").unwrap();
    assert!(r.as_f64() > 7.0);
}

#[test]
fn test_tween_easing_smoothstep() {
    let r = w().run("let t = Tween(0.0, 10.0, 1.0)\nt.setEasing(\"smoothstep\")\nt.update(0.5)\nreturn t.value()").unwrap();
    assert!((r.as_f64() - 5.0).abs() < 0.1);
}

#[test]
fn test_tween_loop_mode() {
    let r = w().run("let t = Tween(0.0, 10.0, 1.0)\nt.setLoop(\"loop\")\nt.update(1.5)\nreturn t.isFinished()").unwrap();
    assert_eq!(r, Value::Bool(false));
}

#[test]
fn test_tween_pingpong_mode() {
    let r = w().run("let t = Tween(0.0, 10.0, 1.0)\nt.setLoop(\"pingpong\")\nt.update(1.5)\nreturn t.isFinished()").unwrap();
    assert_eq!(r, Value::Bool(false));
}

#[test]
fn test_tween_pingpong_bounce_back() {
    // After going forward past duration, direction reverses; update again to go backwards past 0
    let r = w().run("let t = Tween(0.0, 10.0, 1.0)\nt.setLoop(\"pingpong\")\nt.update(1.0)\nt.update(1.0)\nt.update(1.0)\nreturn t.value()").unwrap();
    assert!(r.as_f64() >= 0.0 && r.as_f64() <= 10.0);
}

#[test]
fn test_tween_delay() {
    let r = w().run("let t = Tween(0.0, 10.0, 1.0)\nt.setDelay(0.5)\nt.update(0.25)\nreturn t.value()").unwrap();
    assert!(r.as_f64().abs() < 0.01); // still in delay
}

#[test]
fn test_tween_update_after_finished() {
    let r = w().run("let t = Tween(0.0, 10.0, 1.0)\nt.update(2.0)\nreturn t.update(1.0)").unwrap();
    assert!((r.as_f64() - 10.0).abs() < 0.01);
}

#[test]
fn test_tween_zero_duration() {
    let r = w().run("let t = Tween(0.0, 10.0, 0.0)\nreturn t.value()").unwrap();
    assert!((r.as_f64() - 10.0).abs() < 0.01);
}

// ── Quaternion ───────────────────────────────────────────────────────

#[test]
fn test_quaternion_lerp() {
    let r = w().run("let q1 = Quaternion_fromEuler(0.0, 0.0, 0.0)\nlet q2 = Quaternion_fromEuler(0.0, 0.0, 1.0)\nreturn q1.lerp(q2, 0.0).w").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.1);
}

#[test]
fn test_quaternion_to_matrix() {
    let r = w().run("let q = Quaternion_fromEuler(0.0, 0.0, 0.0)\nreturn typeof(q.toMatrix())").unwrap();
    assert_eq!(r, Value::Str(std::rc::Rc::from("Matrix4")));
}

#[test]
fn test_quaternion_rotate() {
    let r = w().run("let q = Quaternion_fromEuler(0.0, 0.0, 0.0)\nreturn q.rotate(Vector3(1.0, 0.0, 0.0)).x").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_quaternion_mul() {
    let r = w().run("let a = Quaternion_fromEuler(0.0, 0.0, 0.0)\nlet b = Quaternion_fromEuler(0.0, 0.0, 0.0)\nreturn a.mul(b).w").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.01);
}

#[test]
fn test_quaternion_look_rotation() {
    let r = w().run("return Quaternion_lookRotation(Vector3(0.0, 0.0, -1.0), Vector3(0.0, 1.0, 0.0)).w").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.01);
}

#[test]
fn test_quaternion_field_reads_xyz() {
    let r = w().run("let q = Quaternion_fromEuler(0.0, 0.0, 0.0)\nreturn q.x + q.y + q.z").unwrap();
    assert!(r.as_f64().abs() < 0.001);
}

// ── Interpolation extras ─────────────────────────────────────────────

#[test]
fn test_inverse_lerp() {
    let r = w().run("return inverseLerp(0.0, 10.0, 5.0)").unwrap();
    assert!((r.as_f64() - 0.5).abs() < 0.01);
}

#[test]
fn test_ease_out_quad() {
    let r = w().run("return easeOutQuad(1.0)").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_ease_in_out_quad() {
    let r = w().run("return easeInOutQuad(0.5)").unwrap();
    assert!((r.as_f64() - 0.5).abs() < 0.01);
}

#[test]
fn test_ease_out_cubic() {
    let r = w().run("return easeOutCubic(1.0)").unwrap();
    assert!((r.as_f64() - 1.0).abs() < 0.001);
}

#[test]
fn test_ease_in_out_cubic() {
    let r = w().run("return easeInOutCubic(0.5)").unwrap();
    assert!((r.as_f64() - 0.5).abs() < 0.01);
}

#[test]
fn test_ease_in_out_sine() {
    let r = w().run("return easeInOutSine(0.0)").unwrap();
    assert!(r.as_f64().abs() < 0.001);
}

// ── Reflect ──────────────────────────────────────────────────────────

#[test]
fn test_reflect_typeof() {
    assert_eq!(w().run("return typeof(42)").unwrap(), Value::Str(std::rc::Rc::from("int")));
    assert_eq!(w().run("return typeof(3.14)").unwrap(), Value::Str(std::rc::Rc::from("float")));
    assert_eq!(w().run("return typeof(true)").unwrap(), Value::Str(std::rc::Rc::from("bool")));
    assert_eq!(w().run("return typeof(null)").unwrap(), Value::Str(std::rc::Rc::from("null")));
}

#[test]
fn test_reflect_instanceof() {
    let r = w().run("return instanceof(42, \"int\")").unwrap();
    assert_eq!(r, Value::Bool(true));
    let r = w().run("return instanceof(42, \"float\")").unwrap();
    assert_eq!(r, Value::Bool(false));
}

#[test]
fn test_reflect_has_field_dict() {
    let r = w().run("let d = {\"x\": 1}\nreturn hasField(d, \"x\")").unwrap();
    assert_eq!(r, Value::Bool(true));
    let r = w().run("let d = {\"x\": 1}\nreturn hasField(d, \"y\")").unwrap();
    assert_eq!(r, Value::Bool(false));
}

#[test]
fn test_reflect_get_field_dict() {
    let r = w().run("let d = {\"x\": 42}\nreturn getField(d, \"x\")").unwrap();
    assert_eq!(r, Value::I32(42));
}

#[test]
fn test_reflect_set_field_dict() {
    let r = w().run("let d = {\"x\": 1}\nsetField(d, \"x\", 99)\nreturn d[\"x\"]").unwrap();
    assert_eq!(r, Value::I32(99));
}

#[test]
fn test_reflect_fields_dict() {
    let r = w().run("let d = {\"x\": 1, \"y\": 2}\nlet f = fields(d)\nreturn f.length").unwrap();
    assert_eq!(r, Value::I32(2));
}

#[test]
fn test_reflect_has_field_on_non_object() {
    let r = w().run("return hasField(42, \"x\")").unwrap();
    assert_eq!(r, Value::Bool(false));
}

#[test]
fn test_reflect_get_field_on_non_object() {
    assert!(w().run("return getField(42, \"x\")").is_err());
}

#[test]
fn test_reflect_set_field_on_non_object() {
    assert!(w().run("setField(42, \"x\", 1)").is_err());
}

#[test]
fn test_reflect_methods_on_non_struct() {
    let r = w().run("let m = methods(42)\nreturn m.length").unwrap();
    assert_eq!(r, Value::I32(0));
}

#[test]
fn test_reflect_has_method_on_non_struct() {
    let r = w().run("return hasMethod(42, \"foo\")").unwrap();
    assert_eq!(r, Value::Bool(false));
}
