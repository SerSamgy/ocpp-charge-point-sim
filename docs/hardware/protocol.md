# Hardware steps protocol

How a task that touches hardware runs: who does what, when the agent
stops and waits for the owner, how results are verified, and what needs
the owner's approval. It applies to wiring, Pi OS configuration, and
running code that drives pins or serial ports.

The agent cannot touch hardware. The owner wires, powers, measures and
presses the buttons; the agent plans, explains, checks and writes code.

This page covers the process only. It holds no numbers about a
particular board or part: every such fact comes from the official
documentation, fetched when needed and linked from `docs/hardware/`
(see "Records"). `CLAUDE.md` ("Hardware and safety") has the hard limits
and wins on conflict; this page does not repeat them.

Messages to the owner are in Russian with a gloss for every new term
(`CLAUDE.md`, Communication); this page is English because it is
committed.

## Phases

Every hardware step runs in this order:

1. **Explain.** The electrical or system basis, briefly, for someone
   who learned it at school long ago. Show the calculation with numbers
   (for example the resistor of an LED), and name the source of every
   figure.
2. **Prepare.** Parts and tools with an approximate price in Germany
   (`CLAUDE.md`, Purchases), what the owner already has (ask, never
   assume), the recovery path, the safe state of every output, the
   baseline state (see "Session start").
3. **Physical action** by the owner, one logical group at a time.
4. **Verify** with read-only commands and the owner's report.
5. **Code**, only after the hardware checked out.

The plan of a hardware task lists the physical actions separately and
contains a section **Checklist for this step, with sources**: what can go
wrong in this kind of change (levels, polarity, current limits, boot
behaviour, back-feeding between supplies and so on), each point backed by
the official documentation. The checklist is written per step, when its
parts are known; this page does not carry one.

## Session start

At the start of every session of a hardware task, ask what is connected
and powered right now, and compare it with `docs/hardware/`. The hardware
keeps its state between sessions, the agent's memory does not, and the
owner may have moved a wire.

## Stop and wait

Before every physical action the agent writes one message and ends the
turn. No tool calls and no "next steps" follow it. The message holds at
most about five numbered items for one logical action, and for each:

- the required power state;
- the pin or connector: BCM number, physical number, wire colour or
  label, function;
- what to measure with the multimeter before powering, and the expected
  reading;
- what the owner should observe afterwards;
- what exactly to report back (a command output, a number, a photo);
- when to stop and disconnect power (see "Abort conditions").

The agent never assumes an action was done or succeeded. A report that
differs from the expectation stops the step; "it probably works" is not
a result.

**Wiring checkpoint.** Every new or changed wiring on a breadboard ends
with the owner sending a photo and a list "wire from pin X (BCM and
physical) to hole Y (row and column, or label)". The agent compares the
list with the pin table and the plan, and the photo with the list, and
asks about every mismatch or anything the photo does not show. It says
only "matches the plan by the list and the photo"; powering on is the
owner's decision. No power goes to new wiring without this checkpoint.

## Command classes

Commands on the Pi, over `ssh`, belong to one of three classes. Every
`ssh`, `scp` and `just deploy` also goes through the permission prompt
(`.claude/settings.json`, ADR 0005): the agent says in one line what the
command does and what happens physically, and the prompt is the second
barrier. The agent does not ask for or suggest allow rules for them, and
the owner does not run hardware steps in a mode that skips prompts.

**R, read.** A closed list: `pinctrl get`, `gpioinfo`, `ls /dev`,
`ls /dev/serial/by-id`, `cat` of the boot files, `uname`, the contents of
`/etc/os-release`, `journalctl -k` (`dmesg` only if it works without
`sudo`), `systemctl status` and `systemctl cat`, `vcgencmd
get_throttled`. If one of them needs more rights, the agent asks the
owner to run it. Anything not on the list is class A or P.

**A, actuating.** Anything that can change a pin or send bytes: `pinctrl
set`, running `cp-blink` or `cp-sim` on the Pi, capturing output from a
serial port. Before it: the owner confirms that the wiring is final and
the power is on, and the plan says what each pin does. A program is
started under `timeout`, which only limits the time: killing a process
by a signal may leave its pins as they were, so the state afterwards is
checked by observation, and a reset is a command of class A of its own.
`just deploy` only copies a file and is not class A; running the copy
is.

**P, persistent.** Boot files (`config.txt`, `cmdline.txt`; their path
depends on the OS release, confirm it from the documentation), systemd
units, packages, users, network, reboot and
shutdown. The owner runs these. The agent supplies the exact commands,
the backup, the diff and the way back. The agent never escalates
privileges on the Pi over `ssh` (`sudo`, `su`, `doas`, root login): the
local `sudo` deny rule does not see a command inside `ssh`, so this is a
rule of the protocol.

## Reaching the Pi

- Never assume the Pi is on. One attempt only, with a short timeout and
  no prompts (`ssh -o ConnectTimeout=5 -o BatchMode=yes`).
- If it fails, show the error as it is and ask the owner. No retries, no
  network scans, no guessing an address.
- A changed host key (typical after the SD card was reimaged) is the
  owner's decision. The agent does not turn off host key checking and
  does not edit `known_hosts`.

## Powering off for wiring

Wiring changes only with the power off: no wire is added, moved or
removed while the board is powered. The owner shuts the Pi down cleanly
and then unplugs its power cable; a halted board with the cable in is
treated as powered. The agent does not rely on an LED for "off" until the
official documentation has been checked and its statement recorded in
`docs/hardware/`.

## Serial ports and terminal tools

- Interactive terminal programs (`picocom`, `minicom`, `screen`, `tio`)
  are run by the owner.
- The agent never writes to a serial port directly (shell redirection,
  `echo`, terminal programs): the other end may be a logged-in console or
  a controller, so a write is a command. Running project code that talks
  to a serial port is class A.
- Capturing output is class A and goes in this order: the owner confirms
  that their terminal is closed; the port is set without echo (`stty
  -echo raw` or a program that only reads), because a tty echoes what it
  receives by default and a plain `cat` could send the console's own
  output back into it (to be confirmed when the first serial step runs);
  then the read runs under `timeout`, by the path in
  `/dev/serial/by-id/`.
- The agent does not read devices in `/dev` with the Read tool or with
  `cat` without `timeout`: such a read can block.

## Recovery

The Pi boots from a single SD card, so the card is the system and the
only way back. Before the first class P change of a task:

- a full image of the card is on the owner's laptop, outside the
  repository, with its size and hash checked; or a spare card holds a
  working system. The owner writes the image; the agent supplies the
  commands and the `lsblk` checks that keep a write off the wrong disk,
  and never writes to block devices itself;
- the file to change is backed up on the Pi;
- the way back is written down: undoing the change when `ssh` works, and
  when it does not (the boot files are visible from a laptop with a card
  reader, a working serial console is another way in).

Further rules for class P:

- `cmdline.txt` is a single line.
- One change at a time, tested by a reboot while the owner is next to the
  Pi. Not at the end of the evening, and not together with wiring.
- A broken service or filesystem is not repaired through the boot files:
  that is what the image is for.

## Safe state

For every output the plan states what it is during boot, after the
program exits, and after a crash, and that is checked by observation.
Pins that may float during boot, relays and anything driving a load need
this most. At the end of every session the outputs are in their safe
state and nothing runs under load unattended.

## Stop list

Forbidden in this project: mains voltage, a real vehicle or charging
station, batteries.

Only under a separate plan the owner approved, with its own safety
section: more than 5 V on the bench, generating a Control Pilot signal,
a relay with a coil above 5 V, motors, soldering.

Anything else not covered here: stop and say so.

## Abort conditions

A smell, a hot component, smoke, an unexpected reboot or undervoltage
warning, a multimeter reading that does not match the expectation, a pin
that does not do what the plan said: the owner disconnects the power
first, and the conversation starts afterwards.

## Size of a hardware task

One step is one change and one check. Wiring, boot configuration and code
do not share a step. The size of a pull request counts the physical
actions: waiting for the owner is part of the evening.

## Code

- Access to pins and ports sits in a thin module or behind a trait; the
  logic (states, debouncing) is tested on the laptop with no hardware.
- The "How to verify" section of the pull request lists the physical
  checks the owner runs; their results are marked as reported by the
  owner. CI covers no hardware: it only checks that the code builds for
  the Pi target.

## Records

- Committed, in `docs/hardware/`: the pin table with BCM and physical
  numbers, wiring, measurements, the date of each check, and a link to
  the official page every figure comes from. The journal is private and
  its entries are never referenced from the repository, so a fact that
  only lives there is lost to the next session and to the reviewers.
- The journal (`docs/journal/`, Russian, `CLAUDE.md`): the narrative,
  observations, mistakes and what was learned.
