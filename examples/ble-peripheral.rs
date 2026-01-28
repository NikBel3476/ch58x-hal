#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use core::ffi::c_void;
use core::mem::size_of_val;

use ch58x_hal as hal;
use embassy_executor::Spawner;
use embassy_time::{Delay, Duration, Instant, Timer};
use hal::ble::ffi::*;
use hal::ble::gap::*;
use hal::ble::gattservapp::*;
use hal::gpio::{AnyPin, Input, Level, Output, OutputDrive, Pin, Pull};
use hal::interrupt::Interrupt;
use hal::prelude::*;
use hal::rtc::Rtc;
use hal::uart::UartTx;
use hal::{ble, peripherals, println};
use qingke_rt::highcode;

// GAP - SCAN RSP data (max size = 31 bytes)
static mut SCAN_RSP_DATA: &[u8] = &[
    // complete name
    0x12, // length of this data
    GAP_ADTYPE_LOCAL_NAME_COMPLETE as u8,
    b'S',
    b'i',
    b'm',
    b'p',
    b'l',
    b'e',
    b' ',
    b'P',
    b'e',
    b'r',
    b'i',
    b'p',
    b'h',
    b'e',
    b'r',
    b'a',
    b'l',
    // Tx power level
    0x02, // length of this data
    GAP_ADTYPE_POWER_LEVEL as u8,
    0, // 0dBm
];
// GAP - Advertisement data (max size = 31 bytes, though this is
// best kept short to conserve power while advertisting)
static mut ADVERT_DATA: &[u8] = &[
    0x02, // length of this data
    GAP_ADTYPE_FLAGS as u8,
    GAP_ADTYPE_FLAGS_BREDR_NOT_SUPPORTED as u8,
    // https://www.bluetooth.com/specifications/assigned-numbers/
    0x04,                                   // length of this data including the data type byte
    GAP_ADTYPE_MANUFACTURER_SPECIFIC as u8, // manufacturer specific advertisement data type
    0xD7,
    0x07, // 0x07D7, Nanjing Qinheng Microelectronics Co., Ltd.
    0x01,
];

// GAP GATT Attributes
static ATT_DEVICE_NAME: &[u8] = b"Simple Peripheral";

#[embassy_executor::task]
async fn blink(pin: AnyPin) {
    let mut led = Output::new(pin, Level::Low, OutputDrive::_5mA);

    loop {
        led.set_high();
        Timer::after(Duration::from_millis(150)).await;
        led.set_low();
        Timer::after(Duration::from_millis(150)).await;
    }
}

fn peripheral_init() {
    // Setup the GAP Peripheral Role Profile
    unsafe {
        // interval unit 1.25ms
        const MIN_INTERVAL: u16 = 6; // 6*1.25 = 7.5ms
        const MAX_INTERVAL: u16 = 100; // 100*1.25 = 125ms

        // Set the GAP Role Parameters
        GAPRole_SetParameter(GAPROLE_ADVERT_ENABLED as u16, 1, &true as *const _ as _);
        GAPRole_SetParameter(
            GAPROLE_SCAN_RSP_DATA as u16,
            SCAN_RSP_DATA.len() as _,
            SCAN_RSP_DATA.as_ptr() as _,
        );
        GAPRole_SetParameter(
            GAPROLE_ADVERT_DATA as u16,
            ADVERT_DATA.len() as _,
            ADVERT_DATA.as_ptr() as _,
        );
        GAPRole_SetParameter(GAPROLE_MIN_CONN_INTERVAL as u16, 2, &MIN_INTERVAL as *const _ as _);
        GAPRole_SetParameter(GAPROLE_MAX_CONN_INTERVAL as u16, 2, &MAX_INTERVAL as *const _ as _);
    }

    // Set the GAP Characteristics
    unsafe {
        GGS_SetParameter(
            GGS_DEVICE_NAME_ATT as u8,
            ATT_DEVICE_NAME.len() as _,
            ATT_DEVICE_NAME.as_ptr() as _,
        );
    }

    unsafe {
        // units of 625us, 80=50ms
        const ADVERTISING_INTERVAL: u16 = 80;

        // Set advertising interval
        GAP_SetParamValue(TGAP_DISC_ADV_INT_MIN as u16, ADVERTISING_INTERVAL);
        GAP_SetParamValue(TGAP_DISC_ADV_INT_MAX as u16, ADVERTISING_INTERVAL);

        // Enable scan req notify
        GAP_SetParamValue(TGAP_ADV_SCAN_REQ_NOTIFY as u16, 1);
    }

    // Setup the GAP Bond Manager
    unsafe {
        let passkey: u32 = 0; // passkey "000000"
        let pair_mode = GAPBOND_PAIRING_MODE_WAIT_FOR_REQ;
        let mitm = true;
        let bonding = true;
        let io_cap = GAPBOND_IO_CAP_DISPLAY_ONLY;
        GAPBondMgr_SetParameter(
            GAPBOND_PERI_DEFAULT_PASSCODE as u16,
            size_of_val(&passkey) as _,
            &passkey as *const _ as _,
        );
        GAPBondMgr_SetParameter(GAPBOND_PERI_PAIRING_MODE as u16, 1, &pair_mode as *const _ as _);
        GAPBondMgr_SetParameter(GAPBOND_PERI_MITM_PROTECTION as u16, 1, &mitm as *const _ as _);
        GAPBondMgr_SetParameter(GAPBOND_PERI_IO_CAPABILITIES as u16, 1, &io_cap as *const _ as _);
        GAPBondMgr_SetParameter(GAPBOND_PERI_BONDING_ENABLED as u16, 1, &bonding as *const _ as _);
    }

    // Initialize GATT attributes
    unsafe {
        GGS_AddService(GATT_ALL_SERVICES); // GAP
        GATTServApp::add_service(GATT_ALL_SERVICES); // GATT attributes
    }

    // Setup the SimpleProfile Characteristic Values
    // SimpleProfile_SetParameter

    // Register receive scan request callback
    unsafe {
        static mut CB: gapRolesBroadcasterCBs_t = gapRolesBroadcasterCBs_t {
            pfnScanRecv: None,
            pfnStateChange: None,
        };
        GAPRole_BroadcasterSetCB(&mut CB);
    }
}

#[embassy_executor::task]
async fn peripheral(task_id: u8, subscriber: ble::EventSubscriber) {
    unsafe extern "C" fn on_state_change(new_state: gapRole_States_t, event: *mut gapRoleEvent_t) {
        println!("in on_state_change: {}", new_state);
        let event = &*event;

        match new_state {
            GAPROLE_STARTED => {
                println!("initialized..");
            }
            GAPROLE_ADVERTISING => {
                println!("advertising..");
            }
            GAPROLE_WAITING => {
                if event.gap.opcode == GAP_END_DISCOVERABLE_DONE_EVENT as u8 {
                    println!("waiting for advertising..");
                } else if event.gap.opcode == GAP_LINK_TERMINATED_EVENT as u8 {
                    println!("  disconnected .. reason {:x}", event.linkTerminate.reason);
                    // restart advertising here
                    let mut ret: u32 = 0;
                    GAPRole_GetParameter(GAPROLE_ADVERT_ENABLED as u16, &mut ret as *mut _ as *mut c_void);
                    println!("GAPROLE_ADVERT_ENABLED: {}", ret);

                    GAPRole_SetParameter(GAPROLE_ADVERT_ENABLED as u16, 1, &true as *const _ as _);
                } else {
                    println!("unknown event: {}", event.gap.opcode);
                }
            }
            GAPROLE_CONNECTED => {
                println!("connected.. !!");
            }
            GAPROLE_ERROR => {
                println!("error..");
            }
            _ => {
                println!("!!! unknown state: {}", new_state);
            }
        }
    }
    unsafe extern "C" fn on_rssi_read(conn_handle: u16, rssi: i8) {
        println!("RSSI -{} dB Conn {:x}", -rssi, conn_handle);
    }
    unsafe extern "C" fn on_param_update(conn_handle: u16, interval: u16, slave_latency: u16, timeout: u16) {
        println!(
            "on_param_update Conn handle: {} inverval: {} timeout: {}",
            conn_handle, interval, timeout
        );
    }

    unsafe {
        static mut BOND_MGR_CB: gapBondCBs_t = gapBondCBs_t {
            passcodeCB: None,
            pairStateCB: None,
            oobCB: None,
        };

        // peripheralStateNotificationCB

        static mut APP_CB: gapRolesCBs_t = gapRolesCBs_t {
            pfnStateChange: Some(on_state_change),
            pfnRssiRead: Some(on_rssi_read),
            pfnParamUpdate: Some(on_param_update),
        };
        // Start the Device
        GAPRole_PeripheralStartDevice(task_id, &mut BOND_MGR_CB, &mut APP_CB);
    }
}

#[embassy_executor::main(entry = "qingke_rt::entry")]
async fn main(spawner: Spawner) -> ! {
    use hal::ble::ffi::*;

    let mut config = hal::Config::default();
    config.clock.use_pll_60mhz().enable_lse();
    let p = hal::init(config);
    hal::embassy::init();

    let uart = UartTx::new(p.UART1, p.PA9, Default::default()).unwrap();
    unsafe {
        hal::set_default_serial(uart);
    }

    let boot_btn = Input::new(p.PB22, Pull::Up);

    let rtc = Rtc::new(p.RTC);

    println!();
    println!("Hello World from ch58x-hal!");
    println!(
        r#"
    ______          __
   / ____/___ ___  / /_  ____ _____________  __
  / __/ / __ `__ \/ __ \/ __ `/ ___/ ___/ / / /
 / /___/ / / / / / /_/ / /_/ (__  |__  ) /_/ /
/_____/_/ /_/ /_/_.___/\__,_/____/____/\__, /
                                      /____/   on CH582"#
    );
    println!("System Clocks: {}", hal::sysctl::clocks().hclk);
    println!("ChipID: 0x{:02x}", hal::signature::get_chip_id());
    println!("RTC datetime: {}", rtc.now());

    spawner.spawn(blink(p.PA8.degrade())).unwrap();

    // BLE part
    println!("BLE Lib Version: {}", ble::lib_version());

    let (task_id, sub) = hal::ble::init(Default::default()).unwrap();
    println!("BLE task id: {}", task_id);

    unsafe {
        let r = GAPRole_PeripheralInit();
        println!("GAPRole_PeripheralInit: {:?}", r);
    }

    peripheral_init();

    spawner.spawn(peripheral(task_id, sub)).unwrap();

    // Main_Circulation
    mainloop().await
}

#[highcode]
async fn mainloop() -> ! {
    loop {
        Timer::after(Duration::from_micros(300)).await;
        println!("loop");
        unsafe {
            TMOS_SystemProcess();
        }
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;

    let pa9 = unsafe { peripherals::PA9::steal() };
    let uart1 = unsafe { peripherals::UART1::steal() };
    let mut serial = UartTx::new(uart1, pa9, Default::default()).unwrap();

    let _ = writeln!(&mut serial, "\n\n\n{}", info);

    loop {}
}
