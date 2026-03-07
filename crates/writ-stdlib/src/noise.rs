use std::cell::RefCell;

use fastnoise_lite::{FastNoiseLite, FractalType, NoiseType};
use writ_vm::binding::{fn1, fn2, fn3};
use writ_vm::{VM, Value};

thread_local! {
    static NOISE: RefCell<FastNoiseLite> = RefCell::new(FastNoiseLite::new());
}

fn with_noise<F, R>(f: F) -> R
where
    F: FnOnce(&FastNoiseLite) -> R,
{
    NOISE.with(|n| f(&n.borrow()))
}

fn with_noise_mut<F, R>(f: F) -> R
where
    F: FnOnce(&mut FastNoiseLite) -> R,
{
    NOISE.with(|n| f(&mut n.borrow_mut()))
}

pub fn register(vm: &mut VM) {
    vm.register_fn_in_module(
        "noise2D",
        "noise",
        fn2(|x: f64, y: f64| -> Result<f64, String> {
            Ok(with_noise(|n| n.get_noise_2d(x as f32, y as f32)) as f64)
        }),
    );

    vm.register_fn_in_module(
        "noise3D",
        "noise",
        fn3(|x: f64, y: f64, z: f64| -> Result<f64, String> {
            Ok(with_noise(|n| n.get_noise_3d(x as f32, y as f32, z as f32)) as f64)
        }),
    );

    vm.register_fn_in_module(
        "noiseSeed",
        "noise",
        fn1(|seed: f64| -> Result<f64, String> {
            with_noise_mut(|n| n.set_seed(Some(seed as i32)));
            Ok(0.0)
        }),
    );

    vm.register_fn_in_module(
        "noiseType",
        "noise",
        crate::matrix::RawFn(std::rc::Rc::new(|args: &[Value]| {
            let name = match args.first().ok_or("missing type")? {
                Value::Str(s) => s.clone(),
                other => return Err(format!("noiseType expects string, got {:?}", other)),
            };
            let nt = match name.as_str() {
                "perlin" => NoiseType::Perlin,
                "simplex" | "openSimplex2" => NoiseType::OpenSimplex2,
                "openSimplex2S" => NoiseType::OpenSimplex2S,
                "cellular" | "voronoi" => NoiseType::Cellular,
                "value" => NoiseType::Value,
                "valueCubic" => NoiseType::ValueCubic,
                _ => return Err(format!("unknown noise type: {}", name)),
            };
            with_noise_mut(|n| n.set_noise_type(Some(nt)));
            Ok(Value::F64(0.0))
        })),
    );

    vm.register_fn_in_module(
        "noiseFractal",
        "noise",
        fn3(
            |octaves: f64, lacunarity: f64, gain: f64| -> Result<f64, String> {
                with_noise_mut(|n| {
                    n.set_fractal_type(Some(FractalType::FBm));
                    n.set_fractal_octaves(Some(octaves as i32));
                    n.set_fractal_lacunarity(Some(lacunarity as f32));
                    n.set_fractal_gain(Some(gain as f32));
                });
                Ok(0.0)
            },
        ),
    );

    vm.register_fn_in_module(
        "noiseFrequency",
        "noise",
        fn1(|freq: f64| -> Result<f64, String> {
            with_noise_mut(|n| n.set_frequency(Some(freq as f32)));
            Ok(0.0)
        }),
    );
}
