//! GPIO experiments on the Raspberry Pi 5. This binary blinks the LED on
//! GPIO17; the binaries in `src/bin/` read the buttons. Pins:
//! `docs/hardware/README.md`.

use rppal::gpio::Gpio;
use std::{error::Error, thread, time::Duration};

fn main() -> Result<(), Box<dyn Error>> {
    let mut led = Gpio::new()?.get(17)?.into_output();
    loop {
        led.toggle();
        thread::sleep(Duration::from_millis(500));
    }
}
