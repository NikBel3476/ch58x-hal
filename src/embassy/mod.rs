pub mod time_driver_systick;

// This should be called after global clocks inited
pub fn init() {
    critical_section::with(|cs| {
        time_driver_systick::init(cs);
    });

    unsafe {
        crate::gpio::init();
    }
}
