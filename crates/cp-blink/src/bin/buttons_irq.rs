//! Reads the four buttons with GPIO interrupts and the debounce of `rppal`.

use rppal::gpio::{Gpio, Trigger};
use std::sync::mpsc;
use std::time::Duration;

const DEBOUNCE: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, Copy)]
enum Button {
    Cable,
    Authorize,
    Stop,
    Emergency,
}

const BUTTONS: [(Button, u8); 4] = [
    (Button::Cable, 5),
    (Button::Authorize, 6),
    (Button::Stop, 16),
    (Button::Emergency, 26),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gpio = Gpio::new()?;
    let (tx, rx) = mpsc::channel::<Button>();
    let mut pins = Vec::new();

    for (button, bcm) in BUTTONS {
        let mut pin = gpio.get(bcm)?.into_input_pullup();
        let tx = tx.clone();
        pin.set_async_interrupt(Trigger::FallingEdge, Some(DEBOUNCE), move |_event| {
            let _ = tx.send(button);
        })?;
        pins.push(pin);
    }

    println!("Waiting for button presses (interrupts). Ctrl+C to exit.");

    let mut cable_connected = false;
    while let Ok(button) = rx.recv() {
        match button {
            Button::Cable => {
                cable_connected = !cable_connected;
                println!(
                    "Cable: {}",
                    if cable_connected {
                        "connected"
                    } else {
                        "disconnected"
                    }
                );
            }
            other => println!("Event: {other:?}"),
        }
    }

    Ok(())
}
