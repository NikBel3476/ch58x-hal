// #[cfg(all(qingke_v4, not(time_driver_timer)))]
#[path = "time_driver_systick.rs"]
pub mod time_driver_impl;

// #[cfg(time_driver_timer)]
// #[path = "time_driver_tim.rs"]
// pub mod time_driver_impl;

// This should be called after global clocks inited
pub fn init() {
    let p = unsafe { &*crate::pac::PFIC::PTR };
    p.sctlr().write(|w| w.sevonpend().set_bit());

    critical_section::with(|cs| time_driver_impl::init(cs));

    unsafe {
        crate::gpio::init();
    }
}
