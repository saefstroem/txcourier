# txcourier

An air-gapped transaction courier built on a Waveshare RP2350-USB-A.

The device holds at most **one** unsigned transaction and at most **one** signed
transaction. You load an unsigned one from the CLI over the Type-C port,
unplug, carry it to a powerbank, and plug a Ledger Nano S Plus into the USB-A
port; the device asks the Ledger to sign what it is carrying and keeps the
signature until you plug it back into your machine.

It holds no keys and no secrets. Its whole job is to be too simple to lie: a
compromised host cannot make the Ledger sign anything other than a plain value
transfer or an ERC-20 `transfer(address,uint256)`, because the firmware
re-parses and validates the payload itself before it ever reaches the Ledger,
and the Ledger's own screen is the final human-verifiable backstop.

## Layout

```
crates/protocol    wire format, transaction model, validation rules and the
                   lamp policy. Depends only on `thiserror`; compiled into both
                   binaries so there is exactly one definition of what may be
                   signed.
crates/pio-usb     the bit-banged USB host: the three PIO programs, packet
                   encoding and CRC. Builds for the host too, so the wire
                   format is tested without hardware.
crates/firmware    the RP2350 image (riscv32imac, its own workspace)
crates/cli         the macOS command-line tool
```

The code in this repository follows the rules in [CLAUDE.md](CLAUDE.md) — in
particular it carries **no comments at all**: meaning is expected to come from
names, types and the module tree. `python3 .github/scripts/check_rules.py`
enforces that, and the pre-commit hook runs it.

## Building

The firmware targets RISC-V with a custom linker script and lives in its own
workspace, so the two sides build independently with no `--target` juggling:

```bash
python3 .github/scripts/check_rules.py && cargo test --workspace
```

```bash
cd crates/firmware && cargo build --release
```

Flash with the board in BOOTSEL (hold BOOT, tap RESET, release BOOT):

```bash
cd crates/firmware && cargo run --release
```

## The lamp

| Lamp | Meaning |
|---|---|
| Slow blink (1 s on, 1 s off) | Idle: both slots are empty |
| Solid | Holding an unsigned transaction: plug in the Ledger to sign it |
| Fast blink (200 ms on, 200 ms off) | Holding a signed transaction: bring it back to the CLI |
| Dark | Something is wrong: either a fault (power-cycle the device) or both slots are full (clear the signed slot from the CLI) |

## Hardware notes

From the board schematic — several of these are not what you would guess:

- The USB-A port is on **GP12 (D+) / GP13 (D−)**, not the RP2350's own USB
  controller. That one is hardwired to the Type-C connector and cannot be
  routed elsewhere, so the Ledger side is bit-banged over PIO.
- USB-A **VBUS is tied straight to VSYS**, so the Ledger cannot be
  power-cycled in software.
- The onboard WS2812 is on **GP16**; the lamp this firmware drives is an
  external LED on GP0 (anode, through 470 Ω) and GP4 (ground return).
- **R13 has been removed** on this board, making the USB-A port host-only.
- The system clock is **120 MHz**, not the usual 150 — PIO USB needs an exact
  multiple of 12 MHz.

## Enable the pre-commit hook

```bash
git config core.hooksPath .githooks
```
