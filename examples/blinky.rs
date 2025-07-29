#![no_std]
#![no_main]

use ch58x_hal::uart::UartTx;
use embedded_hal_1::delay::DelayNs;
use hal::delay::CycleDelay;
use hal::gpio::{Level, Output, OutputDrive};
use qingke::riscv;
use {ch58x_hal as hal, panic_halt as _};

use core::arch::{asm, global_asm};
use core::fmt::Write;
use core::writeln;

#[qingke_rt::entry]
fn main() -> ! {
    let mut config = hal::Config::default();
    config.clock.use_pll_60mhz().enable_lse();
    let p = hal::init(config);

    let mut delay = CycleDelay;

    // LED PA8
    let mut led = Output::new(p.PA8, Level::High, OutputDrive::_5mA);
    // let mut led = Output::new(p.PB18, Level::Low, OutputDrive::_5mA);
    let mut serial = UartTx::new(p.UART1, p.PA9, Default::default()).unwrap();
    // serial.blocking_flush().unwrap();

    loop {
        led.toggle();
        writeln!(serial, "Hello").unwrap();

        hal::delay_ms(1000);
    }
}
