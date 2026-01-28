// //! SysTick-based time driver.

// use core::cell::{Cell, RefCell};
// use core::{mem, ptr};

// use critical_section::{CriticalSection, Mutex};
// use embassy_time_driver::{AlarmHandle, Driver};
// // use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};
// use portable_atomic::{AtomicU32, AtomicU8, Ordering};

// // use super::AlarmState;
// use crate::pac;

// pub const ALARM_COUNT: usize = 1;

// struct AlarmState {
//     timestamp: Cell<u64>,

//     // This is really a Option<(fn(*mut ()), *mut ())>
//     // but fn pointers aren't allowed in const yet
//     callback: Cell<*const ()>,
//     ctx: Cell<*mut ()>,
// }

// unsafe impl Send for AlarmState {}

// impl AlarmState {
//     const fn new() -> Self {
//         Self {
//             timestamp: Cell::new(u64::MAX),
//             callback: Cell::new(ptr::null()),
//             ctx: Cell::new(ptr::null_mut()),
//         }
//     }
// }

// pub struct SystickDriver {
//     alarm_count: RefCell<u8>,
//     alarms: Mutex<[AlarmState; ALARM_COUNT]>,
//     period: AtomicU32,
// }

// const ALARM_STATE_NEW: AlarmState = AlarmState::new();
// embassy_time_driver::time_driver_impl!(static DRIVER: SystickDriver = SystickDriver {
//     period: AtomicU32::new(1), // avoid div by zero
//     alarm_count: RefCell::new(0),
//     alarms: Mutex::new([ALARM_STATE_NEW; ALARM_COUNT]),
// });

// impl SystickDriver {
//     fn init(&'static self) {
//         let rb = unsafe { &*pac::SYSTICK::PTR };
//         let hclk = crate::sysctl::clocks().hclk.to_Hz() as u64;

//         let cnt_per_second = hclk / 8;
//         let cnt_per_tick = cnt_per_second / embassy_time_driver::TICK_HZ;

//         self.period.store(cnt_per_tick as u32, Ordering::Relaxed);

//         // UNDOCUMENTED:  Avoid initial interrupt
//         rb.cmp().write(|w| unsafe { w.bits(u64::MAX - 1) });
//         critical_section::with(|_| {
//             rb.sr().write(|w| w.cntif().bit(false)); // clear
//                                                      // Configration: Upcount, No reload, HCLK/8 as clock source
//             rb.ctlr().modify(|_, w| {
//                 w.init()
//                     .set_bit()
//                     .mode()
//                     .upcount()
//                     .stre()
//                     .clear_bit()
//                     .stclk()
//                     .hclk_div8()
//                     .ste()
//                     .set_bit()
//             });
//         })
//     }

//     fn on_interrupt(&self) {
//         let rb = unsafe { &*pac::SYSTICK::PTR };
//         rb.ctlr().modify(|_, w| w.stie().clear_bit()); // disable interrupt
//         rb.sr().write(|w| w.cntif().bit(false)); // clear IF

//         critical_section::with(|cs| {
//             self.trigger_alarm(cs);
//         });
//     }

//     fn trigger_alarm(&self, cs: CriticalSection) {
//         let alarm = &self.alarms.borrow(cs)[0];
//         alarm.timestamp.set(u64::MAX);

//         // Call after clearing alarm, so the callback can set another alarm.

//         // safety:
//         // - we can ignore the possiblity of `f` being unset (null) because of the safety contract of `allocate_alarm`.
//         // - other than that we only store valid function pointers into alarm.callback
//         let f: fn(*mut ()) = unsafe { mem::transmute(alarm.callback.get()) };
//         f(alarm.ctx.get());
//     }

//     fn get_alarm<'a>(&'a self, cs: CriticalSection<'a>, alarm: AlarmHandle) -> &'a AlarmState {
//         // safety: we're allowed to assume the AlarmState is created by us, and
//         // we never create one that's out of bounds.
//         unsafe { self.alarms.borrow(cs).get_unchecked(alarm.id() as usize) }
//     }
// }

// impl Driver for SystickDriver {
//     fn now(&self) -> u64 {
//         let rb = unsafe { &*pac::SYSTICK::PTR };
//         rb.cnt().read().bits() / (self.period.load(Ordering::Relaxed) as u64)
//     }
//     unsafe fn allocate_alarm(&self) -> Option<AlarmHandle> {
//         // let id = self.alarm_count.fetch_update(Ordering::AcqRel, Ordering::Acquire, |x| {
//         //     if x < ALARM_COUNT as u8 {
//         //         Some(x + 1)
//         //     } else {
//         //         None
//         //     }
//         // });

//         // match id {
//         //     Ok(id) => Some(AlarmHandle::new(id)),
//         //     Err(_) => None,
//         // }

//         self.alarm_count = self.alarm_count + 1;
//         if self.alarm_count - 1 < ALARM_COUNT as u8 {
//             Some(AlarmHandle::new(self.alarm_count - 1))
//         } else {
//             None
//         }
//     }
//     fn set_alarm_callback(&self, alarm: AlarmHandle, callback: fn(*mut ()), ctx: *mut ()) {
//         critical_section::with(|cs| {
//             let alarm = self.get_alarm(cs, alarm);

//             alarm.callback.set(callback as *const ());
//             alarm.ctx.set(ctx);
//         })
//     }
//     fn set_alarm(&self, alarm: AlarmHandle, timestamp: u64) -> bool {
//         critical_section::with(|cs| {
//             let _n = alarm.id();

//             let alarm = self.get_alarm(cs, alarm);
//             alarm.timestamp.set(timestamp);

//             let rb = unsafe { &*pac::SYSTICK::PTR };

//             let t = self.now();
//             if timestamp <= t {
//                 // If alarm timestamp has passed the alarm will not fire.
//                 // Disarm the alarm and return `false` to indicate that.
//                 rb.ctlr().modify(|_, w| w.stie().clear_bit());

//                 alarm.timestamp.set(u64::MAX);

//                 return false;
//             }

//             let safe_timestamp = (timestamp + 1) * (self.period.load(Ordering::Relaxed) as u64);

//             rb.cmp().write(|w| unsafe { w.bits(safe_timestamp) });
//             rb.ctlr().modify(|_, w| w.stie().set_bit());

//             true
//         })
//     }
// }

// #[allow(non_snake_case)]
// #[link_section = ".trap"]
// #[no_mangle]
// extern "C" fn SysTick() {
//     DRIVER.on_interrupt();
// }

// pub(crate) fn init() {
//     use qingke::interrupt::Priority;
//     use qingke_rt::CoreInterrupt;

//     DRIVER.init();

//     unsafe {
//         qingke::pfic::set_priority(CoreInterrupt::SysTick as u8, Priority::P15 as _);
//         qingke::pfic::enable_interrupt(CoreInterrupt::SysTick as u8);
//     }
// }

//! SysTick-based time driver.

use core::cell::RefCell;
use core::sync::atomic::{AtomicU32, Ordering};

use critical_section::CriticalSection;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_time_driver::Driver;
use embassy_time_queue_utils::Queue;
use qingke_rt::interrupt;

use crate::pac;

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
        while !self.set_alarm(cs, next) {
            next = self.queue.borrow(cs).borrow_mut().next_expiration(self.raw_cnt());
        }
    }

    #[inline]
    fn raw_cnt(&self) -> u64 {
        let rb = unsafe { &*pac::SYSTICK::PTR };
        rb.cnt().read().bits()
    }

    fn set_alarm(&self, cs: critical_section::CriticalSection, next_alarm_cnt: u64) -> bool {
        critical_section::with(|cs| {
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
        })
    }
}

impl Driver for SystickDriver {
    fn now(&self) -> u64 {
        self.raw_cnt() / (self.cnt_per_tick.load(Ordering::Relaxed) as u64)
    }

    fn schedule_wake(&self, ticks: u64, waker: &core::task::Waker) {
        // let cnt_per_tick = self.cnt_per_tick.load(Ordering::Relaxed) as u64;
        critical_section::with(|cs| {
            let mut queue = self.queue.borrow(cs).borrow_mut();
            if queue.schedule_wake(ticks /* * cnt_per_tick */, waker) {
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
