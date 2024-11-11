// Copyright 2024 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![no_std]
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(unsafe_op_in_unsafe_fn)]

use bitflags::bitflags;
use core::fmt;
use core::hint::spin_loop;
use core::ptr::{addr_of, addr_of_mut};
use embedded_io::{ErrorKind, ErrorType, Read, ReadReady, Write, WriteReady};

bitflags! {
    /// Flags from Data Register
    #[repr(transparent)]
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    struct Data: u16 {
        /// Data character.
        const DATA = 0b11111111;
        /// Framing error.
        const FE = 1 << 8;
        /// Parity error.
        const PE = 1 << 9;
        /// Break error.
        const BE = 1 << 10;
        /// Overrun error.
        const OE = 1 << 11;
    }
}

bitflags! {
    /// Flags from the UART flag register.
    #[repr(transparent)]
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    struct Flags: u16 {
        /// Clear to send.
        const CTS = 1 << 0;
        /// Data set ready.
        const DSR = 1 << 1;
        /// Data carrier detect.
        const DCD = 1 << 2;
        /// UART busy transmitting data.
        const BUSY = 1 << 3;
        /// Receive FIFO is empty.
        const RXFE = 1 << 4;
        /// Transmit FIFO is full.
        const TXFF = 1 << 5;
        /// Receive FIFO is full.
        const RXFF = 1 << 6;
        /// Transmit FIFO is empty.
        const TXFE = 1 << 7;
        /// Ring indicator.
        const RI = 1 << 8;
    }
}

bitflags! {
    /// Flags from the UART Receive Status Register / Error Clear Register.
    #[repr(transparent)]
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    struct ReceiveStatus: u16 {
        /// Framing error.
        const FE = 1 << 0;
        /// Parity error.
        const PE = 1 << 1;
        /// Break error.
        const BE = 1 << 2;
        /// Overrun error.
        const OE = 1 << 3;
    }
}

bitflags! {
    /// Flags from the UART Control Register.
    #[repr(transparent)]
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    struct Control: u16 {
        /// UART Enable.
        const UARTEN = 1 << 0;
        /// Serial InfraRed (SIR) Enable.
        const SIREN = 1 << 1;
        /// Serial InfraRed (SIR) Low-power.
        const SIRLP = 1 << 2;
        /// Bits 6:3 are reserved.
        /// Loopback Enable.
        const LBE = 1 << 7;
        /// Transmit Enable.
        const TXE = 1 << 8;
        /// Receive Enable.
        const RXE = 1 << 9;
        /// Data Transmit Ready.
        const DTR = 1 << 10;
        /// Request To Send.
        const RTS = 1 << 11;
        /// Complement of nUARTOut1
        const OUT1 = 1 << 12;
        /// Complement of nUARTOut2
        const OUT2 = 1 << 13;
        /// Request To Send (RTS) Hardware Flow Control Enable.
        const RTSEN = 1 << 14;
        /// Clear To Send (CTS) Hardware Flow Control Enable.
        const CTSEN = 1 << 15;
    }
}

#[repr(C, align(4))]
struct Registers {
    /// Data Register.
    dr: u16,
    _reserved0: [u8; 2],
    /// Receive Status Register / Error Clear Register.
    rsr: ReceiveStatus,
    _reserved1: [u8; 19],
    /// Flag Register.
    fr: Flags,
    _reserved2: [u8; 6],
    /// IrDA Low-Power Counter Register.
    ilpr: u8,
    _reserved3: [u8; 3],
    /// Integer Baud Rate Register.
    ibrd: u16,
    _reserved4: [u8; 2],
    /// Fractional Baud Rate Register.
    fbrd: u8,
    _reserved5: [u8; 3],
    /// Line Control Register.
    lcr_h: u8,
    _reserved6: [u8; 3],
    /// Control Register.
    cr: Control,
    _reserved7: [u8; 3],
    /// Interrupt FIFO Level Select Register.
    ifls: u8,
    _reserved8: [u8; 3],
    /// Interrupt Mask Set/Clear Register.
    imsc: u16,
    _reserved9: [u8; 2],
    /// Raw Interrupt Status Register.
    ris: u16,
    _reserved10: [u8; 2],
    /// Masked Interrupt Status Register.
    mis: u16,
    _reserved11: [u8; 2],
    /// Interrupt Clear Register.
    icr: u16,
    _reserved12: [u8; 2],
    /// DMA Control Register.
    dmacr: u8,
    _reserved13: [u8; 3],
}

/// Errors which may occur reading from a PL011 UART.
#[derive(Copy, Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    /// Break condition detected.
    #[error("Break condition detected")]
    Break,
    /// The received character did not have a valid stop bit.
    #[error("Framing error, received character didn't have a valid stop bit")]
    Framing,
    /// Data was received while the FIFO was already full.
    #[error("Overrun, data received while the FIFO was already full")]
    Overrun,
    /// Parity of the received data character did not match the selected parity.
    #[error("Parity of the received data character did not match the selected parity")]
    Parity,
}

impl embedded_io::Error for Error {
    fn kind(&self) -> ErrorKind {
        match self {
            Self::Break | Self::Overrun => ErrorKind::Other,
            Self::Framing | Self::Parity => ErrorKind::InvalidData,
        }
    }
}

/// Driver for a PL011 UART.
#[derive(Debug)]
pub struct Uart {
    registers: *mut Registers,
}

impl Uart {
    /// Constructs a new instance of the UART driver for a PL011 device at the
    /// given base address.
    ///
    /// # Safety
    ///
    /// The given base address must point to the 14 MMIO control registers of a
    /// PL011 device, which must be mapped into the address space of the process
    /// as device memory and not have any other aliases.
    pub unsafe fn new(base_address: *mut u32) -> Self {
        Self {
            registers: base_address as *mut Registers,
        }
    }

    /// Initializes PL011 UART.
    ///
    /// clock: Uart clock in Hz.
    /// baud_rate: Baud rate.
    pub fn init(&mut self, clock: u32, baud_rate: u32) {
        let divisor = (clock << 2) / baud_rate;

        // SAFETY: self.registers points to the control registers of a PL011 device which is
        // appropriately mapped, as promised by the caller of `Uart::new`.
        unsafe {
            // Disable UART before programming.
            let mut cr: Control = addr_of_mut!((*self.registers).cr).read_volatile();
            cr &= !Control::UARTEN;
            addr_of_mut!((*self.registers).cr).write_volatile(cr);
            // Program Integer Baud Rate.
            addr_of_mut!((*self.registers).ibrd).write_volatile((divisor >> 6).try_into().unwrap());
            // Program Fractional Baud Rate.
            addr_of_mut!((*self.registers).fbrd)
                .write_volatile((divisor & 0x3F).try_into().unwrap());
            // Clear any pending errors.
            addr_of_mut!((*self.registers).rsr).write_volatile(ReceiveStatus::empty());
            // Enable UART.
            addr_of_mut!((*self.registers).cr)
                .write_volatile(Control::RXE | Control::TXE | Control::UARTEN);
        }
    }

    /// Writes a single byte to the UART.
    ///
    /// This blocks until there is space in the transmit FIFO or holding register, but returns as
    /// soon as the byte has been written to the transmit FIFO or holding register. It doesn't wait
    /// for the byte to be sent.
    pub fn write_byte(&mut self, byte: u8) {
        // Wait until there is room in the TX buffer.
        while self.flags().contains(Flags::TXFF) {
            spin_loop();
        }

        // SAFETY: self.registers points to the control registers of a PL011 device which is
        // appropriately mapped, as promised by the caller of `Uart::new`.
        unsafe {
            // Write to the TX buffer.
            addr_of_mut!((*self.registers).dr).write_volatile(u16::from(byte));
        }
    }

    /// Returns whether the UART is currently transmitting data.
    ///
    /// This will be true immediately after calling [`write_byte`](Self::write_byte).
    pub fn is_transmitting(&self) -> bool {
        self.flags().contains(Flags::BUSY)
    }

    /// Reads and returns a pending byte, or `None` if nothing has been
    /// received.
    pub fn read_byte(&mut self) -> Result<Option<u8>, Error> {
        if self.flags().contains(Flags::RXFE) {
            Ok(None)
        } else {
            // SAFETY: self.registers points to the control registers of a PL011 device which is
            // appropriately mapped, as promised by the caller of `Uart::new`.
            let data = unsafe { addr_of!((*self.registers).dr).read_volatile() };
            let error_status = Data::from_bits_truncate(data);
            if error_status.contains(Data::FE) {
                return Err(Error::Framing);
            }
            if error_status.contains(Data::PE) {
                return Err(Error::Parity);
            }
            if error_status.contains(Data::BE) {
                return Err(Error::Break);
            }
            if error_status.contains(Data::OE) {
                return Err(Error::Overrun);
            }
            Ok(Some(data as u8))
        }
    }

    fn flags(&self) -> Flags {
        // SAFETY: self.registers points to the control registers of a PL011 device which is
        // appropriately mapped, as promised by the caller of `Uart::new`.
        unsafe { addr_of!((*self.registers).fr).read_volatile() }
    }
}

impl fmt::Write for Uart {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.as_bytes() {
            self.write_byte(*c);
        }
        Ok(())
    }
}

// SAFETY: `Uart` just contains a pointer to device memory, which can be accessed from any context.
unsafe impl Send for Uart {}

// SAFETY: Methods on `&Uart` don't allow changing any state so are safe to call concurrently from
// any context.
unsafe impl Sync for Uart {}

impl ErrorType for Uart {
    type Error = Error;
}

impl Write for Uart {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        if buf.is_empty() {
            Ok(0)
        } else {
            self.write_byte(buf[0]);
            Ok(1)
        }
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        while self.is_transmitting() {
            spin_loop();
        }
        Ok(())
    }
}

impl WriteReady for Uart {
    fn write_ready(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.flags().contains(Flags::TXFF))
    }
}

impl Read for Uart {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        if buf.is_empty() {
            return Ok(0);
        }

        loop {
            if let Some(byte) = self.read_byte()? {
                buf[0] = byte;
                return Ok(1);
            }
        }
    }
}

impl ReadReady for Uart {
    fn read_ready(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.flags().contains(Flags::RXFE))
    }
}
