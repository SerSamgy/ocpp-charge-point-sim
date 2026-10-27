use rppal::gpio::Gpio;
use std::{error::Error, thread, time::Duration};

fn main() -> Result<(), Box<dyn Error>> {
    let mut led = Gpio::new()?.get(17)?.into_output();
    loop {
        led.toggle();
        thread::sleep(Duration::from_millis(500));
    }
}
