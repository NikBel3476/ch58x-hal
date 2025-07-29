//! SysTick-based time driver.

use core::cell::{RefCell};
use core::sync::atomic::{AtomicU32, Ordering};

use critical_section::CriticalSection;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_time_queue_utils::Queue;
use embassy_time_driver::{Driver};
use qingke_rt::interrupt;

use crate::pac;

pub static mut COUNT: u64 = 0;
pub static mut NEXT: u64 = 0;
pub static mut NEXT_ALARM: u64 = 0;
pub static mut SW_NEXT: u64 = 0;

pub struct SystickDriver {
    cnt_per_tick: AtomicU32,
    queue: Mutex<CriticalSectionRawMutex, RefCell<Queue>>,
}

embassy_time_driver::time_driver_impl!(static DRIVER: SystickDriver = SystickDriver {
    cnt_per_tick: AtomicU32::new(1), // avoid div by zero
    queue: Mutex::new(RefCell::new(Queue::new()))
});

impl SystickDriver {
    fn init(&'static self, _cs: critical_section::CriticalSection) {
        let rb = unsafe { &*pac::SYSTICK::PTR };
        let hclk = crate::sysctl::clocks().hclk.to_Hz() as u64;

        let cnt_per_second = hclk / 8;
        let cnt_per_tick = cnt_per_second / embassy_time_driver::TICK_HZ;

        self.cnt_per_tick.store(cnt_per_tick as u32, Ordering::Relaxed);

        unsafe { rb.cmp().write(|w| w.bits(0)) };
        rb.sr().write(|w| w.cntif().clear_bit());

        // Configration: Upcount, No reload, HCLK/8 as clock source
        rb.ctlr().modify(|_, w| {
            w.init()
                .set_bit()
                .mode()
                .upcount()
                .stre()
                .clear_bit()
                .stclk()
                .hclk_div8()
                .ste()
                .set_bit()
        });
    }

    fn on_interrupt(&self) {
        let rb = unsafe { &*pac::SYSTICK::PTR };
        rb.sr().write(|w| w.cntif().clear_bit()); // clear IF

        critical_section::with(|cs| {
            self.trigger_alarm(cs);
        });
    }

    fn trigger_alarm(&self, cs: CriticalSection) {
        let mut next = self.queue.borrow(cs).borrow_mut().next_expiration(self.raw_cnt());
        unsafe { NEXT = next };
        while !self.set_alarm(cs, next) {
            next = self.queue.borrow(cs).borrow_mut().next_expiration(self.raw_cnt());
            unsafe { NEXT = next };
        }
    }

    #[inline]
    fn raw_cnt(&self) -> u64 {
        let rb = unsafe { &*pac::SYSTICK::PTR };
        rb.cnt().read().bits()
    }

    fn set_alarm(&self, _cs: critical_section::CriticalSection, next_alarm_cnt: u64) -> bool {
        let rb = unsafe { &*pac::SYSTICK::PTR };

        if next_alarm_cnt <= self.raw_cnt() {
            return false;
        }

        rb.cmp().write(|w| unsafe { w.bits(next_alarm_cnt) });
        rb.ctlr().modify(|_, w| w.stie().set_bit());
        rb.sr().write(|w| w.cntif().clear_bit());

        if next_alarm_cnt <= self.raw_cnt() {
            rb.ctlr().modify(|_, w| w.stie().clear_bit());
            rb.sr().write(|w| w.cntif().clear_bit());
            return false;
        }

        true
    }
}

impl Driver for SystickDriver {
    fn now(&self) -> u64 {
        self.raw_cnt() / (self.cnt_per_tick.load(Ordering::Relaxed) as u64)
    }

    fn schedule_wake(&self, ticks: u64, waker: &core::task::Waker) {
        critical_section::with(|cs| {
            let mut queue = self.queue.borrow(cs).borrow_mut();
            if queue.schedule_wake(ticks, waker) {
                let mut next = queue.next_expiration(self.raw_cnt());
                while !self.set_alarm(cs, next) {
                    next = queue.next_expiration(self.raw_cnt());
                }
            }
        })
    }
}

#[interrupt(core)]
fn SysTick() {
    unsafe { COUNT += 1 };
    DRIVER.on_interrupt();
}

pub(crate) fn init(cs: critical_section::CriticalSection) {
    use qingke::interrupt::Priority;
    use qingke_rt::CoreInterrupt;

    DRIVER.init(cs);

    unsafe {
        qingke::pfic::set_priority(CoreInterrupt::SysTick as u8, Priority::P15 as _);
        qingke::pfic::enable_interrupt(CoreInterrupt::SysTick as u8);
    }
}
