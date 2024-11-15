# pl011-uart

[![crates.io page](https://img.shields.io/crates/v/pl011-uart.svg)](https://crates.io/crates/pl011-uart)
[![docs.rs page](https://docs.rs/pl011-uart/badge.svg)](https://docs.rs/pl011-uart)

A Rust driver for the Arm [PL011 UART](https://developer.arm.com/documentation/ddi0183/latest/).

This is not an officially supported Google product.

## Usage

Basic usage of the pl011-uart crate

```rust
use pl011_uart::Uart;
use core::fmt::Write;

fn main() {
    // constants required for initializing the UART.
    const PL011_BASE_ADDRESS: *mut u32 = 0x0900_0000 as _;
    const PL011_BAUD_RATE: u32 = 115200;
    const PL011_CLK_IN_HZ: u32 = 50000000;

    // initialize PL011 UART.
    let mut uart = unsafe { Uart::new(PL011_BASE_ADDRESS) };
    uart.init(PL011_CLK_IN_HZ, PL011_BAUD_RATE);

    // write to PL011 UART.
    writeln!(uart, "Hello, World!").unwrap();
}
```

## License

Licensed under either of

- Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license
  ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributing

If you want to contribute to the project, see details of
[how we accept contributions](CONTRIBUTING.md).
