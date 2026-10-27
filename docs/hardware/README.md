# Hardware

Raspberry Pi 5. Pin numbers are BCM (GPIO numbers of the chip, not the
physical header pins), as used by `rppal` in `crates/cp-blink`.

| Function | GPIO | Mode |
|---|---|---|
| LED | 17 | output |
| Cable button | 5 | input, pull-up |
| Authorize button | 6 | input, pull-up |
| Stop button | 16 | input, pull-up |
| Emergency button | 26 | input, pull-up |

Buttons are active-low: a press pulls the pin to ground and reads as
`Low`. The Cable button toggles a "connected" state; the others are plain
events.

Debouncing window is 20 ms in both variants:

- `buttons`: polling every 1 ms with a software `Debouncer`
- `buttons_irq`: falling-edge interrupts with `rppal`'s debounce option
- `bounce`: prints the raw edges of one press on GPIO5 to see the bounce
