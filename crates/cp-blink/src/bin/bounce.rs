//! Records the raw edges of one button press to make contact bounce visible.

use std::time::{Duration, Instant};

use rppal::gpio::{Gpio, Level};

const PIN: u8 = 5; // GPIO5, "cable" button
const QUIET: Duration = Duration::from_millis(300);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pin = Gpio::new()?.get(PIN)?.into_input_pullup();
    let mut last = pin.read();
    let mut edges: Vec<(Instant, Level)> = Vec::new();

    println!("GPIO{PIN}: press the button. Ctrl+C to exit.");
    loop {
        let level = pin.read();
        let now = Instant::now();
        if level != last {
            edges.push((now, level));
            last = level;
        } else if let Some(&(t_last, _)) = edges.last()
            && now.duration_since(t_last) > QUIET
        {
            report(&edges);
            edges.clear();
        }
    }
}

fn report(edges: &[(Instant, Level)]) {
    let t0 = edges[0].0;
    let parts: Vec<String> = edges
        .iter()
        .map(|(t, l)| {
            let arrow = if *l == Level::Low { "↓" } else { "↑" };
            format!("{arrow}{}mcs", t.duration_since(t0).as_micros())
        })
        .collect();
    println!("{} edge(s): {}", edges.len(), parts.join(" "));
    // let low_edges = edges.iter().filter(|(_, l)| *l == Level::Low).count();
    // println!("low edge(s): {}", low_edges);
}
