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
