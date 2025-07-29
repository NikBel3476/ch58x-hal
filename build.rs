use std::path::PathBuf;
use std::{env, fs};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let mut singletons: Vec<String> = Vec::new();

    let time_driver = match env::vars()
        .map(|(a, _)| a)
        .filter(|x| x.starts_with("CARGO_FEATURE_TIME_DRIVER_"))
        .get_one()
    {
        Ok(x) => Some(
            x.strip_prefix("CARGO_FEATURE_TIME_DRIVER_")
                .unwrap()
                .to_ascii_lowercase(),
        ),
        Err(GetOneError::None) => None,
        Err(GetOneError::Multiple) => panic!("Multiple ch32xx Cargo features enabled"),
    };

    let time_driver_singleton = match time_driver.as_ref().map(|x| x.as_ref()) {
        None => "",
        Some("tim1") => "TIM1",
        Some("tim2") => "TIM2",
        Some("tim3") => "TIM3",
        Some("tim4") => "TIM4",
        Some("tim5") => "TIM5",
        // TIM6 and TIM7 are BCTM
        Some("tim8") => "TIM8",
        Some("tim9") => "TIM9",
        Some("tim10") => "TIM10",
        Some("any") => {
            [
                "TIM5", "TIM4", "TIM3", "TIM2", // GP16 / GP32
                "TIM10", "TIM9", "TIM8", "TIM1", //ADV
            ]
            .iter()
            .find(|tim| singletons.contains(&tim.to_string()))
            .expect("time-driver-any requested, but the chip doesn't have a TIMx for time driver")
        }
        _ => panic!("unknown time_driver {:?}", time_driver),
    };

    if !time_driver_singleton.is_empty() {
        println!("cargo:rustc-cfg=time_driver_{}", time_driver_singleton.to_lowercase());
        println!("cargo:rustc-cfg=time_driver_timer");
    }

    fs::write(out_dir.join("libISP583.a"), include_bytes!("vendor/libISP583.a")).unwrap();

    if env::var("CARGO_FEATURE_BLE").is_ok() {
        fs::write(out_dir.join("libCH58xBLE.a"), include_bytes!("vendor/LIBCH58xBLE.a")).unwrap();
    }

    fs::write(out_dir.join("memory.x"), include_bytes!("memory.x")).unwrap();
    println!("cargo:rustc-link-search={}", out_dir.display());

    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=build.rs");
}

enum GetOneError {
    None,
    Multiple,
}

trait IteratorExt: Iterator {
    fn get_one(self) -> Result<Self::Item, GetOneError>;
}

impl<T: Iterator> IteratorExt for T {
    fn get_one(mut self) -> Result<Self::Item, GetOneError> {
        match self.next() {
            None => Err(GetOneError::None),
            Some(res) => match self.next() {
                Some(_) => Err(GetOneError::Multiple),
                None => Ok(res),
            },
        }
    }
}
