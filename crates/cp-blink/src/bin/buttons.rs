use rppal::gpio::{Gpio, InputPin, Level};
use std::thread;
use std::time::{Duration, Instant};

const DEBOUNCE: Duration = Duration::from_millis(20);
const POLL: Duration = Duration::from_millis(1);
const BUTTONS: [(Button, u8); 4] = [
    (Button::Cable, 5),
    (Button::Authorize, 6),
    (Button::Stop, 16),
    (Button::Emergency, 26),
];

/// Accepts a new level only after it has been stable for `window`.
struct Debouncer {
    stable: Level,
    candidate: Level,
    since: Instant,
    window: Duration,
}

impl Debouncer {
    fn new(initial: Level, window: Duration, now: Instant) -> Self {
        Self {
            stable: initial,
            candidate: initial,
            since: now,
            window,
        }
    }

    /// Returns Some(level) when the stable state changes.
    fn update(&mut self, raw: Level, now: Instant) -> Option<Level> {
        if raw != self.candidate {
            // level changed: restart the timer
            self.candidate = raw;
            self.since = now;
            return None;
        }
        if self.candidate != self.stable && now.duration_since(self.since) >= self.window {
            self.stable = self.candidate;
            return Some(self.stable);
        }
        None
    }
}

#[derive(Debug, Clone, Copy)]
enum Button {
    Cable,
    Authorize,
    Stop,
    Emergency,
}

struct Input {
    button: Button,
    pin: InputPin,
    debouncer: Debouncer,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gpio = Gpio::new()?;
    let start = Instant::now();

    let mut inputs = Vec::new();
    for (button, bcm) in BUTTONS {
        let pin = gpio.get(bcm)?.into_input_pullup();
        let initial = pin.read();
        inputs.push(Input {
            button,
            pin,
            debouncer: Debouncer::new(initial, DEBOUNCE, start),
        });
    }

    let mut cable_connected = false;
    println!("Waiting for button presses. Ctrl+C to exit.");

    loop {
        let now = Instant::now();
        for input in &mut inputs {
            let raw = input.pin.read();
            // active-low: Low = pressed; react only to presses
            if let Some(Level::Low) = input.debouncer.update(raw, now) {
                match input.button {
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
        }
        thread::sleep(POLL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounce_is_ignored() {
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        let mut d = Debouncer::new(Level::High, Duration::from_millis(20), t0);

        assert_eq!(d.update(Level::Low, ms(0)), None); // press...
        assert_eq!(d.update(Level::High, ms(1)), None); // ...bounce
        assert_eq!(d.update(Level::Low, ms(2)), None); // contact again
        assert_eq!(d.update(Level::Low, ms(21)), None); // 19 ms elapsed: too early
        assert_eq!(d.update(Level::Low, ms(22)), Some(Level::Low)); // 20 ms: accepted
        assert_eq!(d.update(Level::Low, ms(50)), None); // nothing new
    }
}
