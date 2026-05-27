#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_time::Timer;
use pico_hid::led::{LED_TICK_MS, led_output};

mod serial;
mod usb;

#[cfg(not(feature = "defmt"))]
use panic_halt as _;
#[cfg(feature = "defmt")]
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let peripherals = embassy_rp::init(Default::default());
    let usb_status = serial::spawn_usb(peripherals.USB, &spawner);
    let mut led = Output::new(peripherals.PIN_25, Level::Low);
    let mut now_ms = 0u32;

    loop {
        let output = led_output(usb_status.led_inputs(now_ms));
        if output.on {
            led.set_high();
        } else {
            led.set_low();
        }
        now_ms = now_ms.wrapping_add(LED_TICK_MS as u32);
        Timer::after_millis(LED_TICK_MS).await;
    }
}
