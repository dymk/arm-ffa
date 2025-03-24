// SPDX-FileCopyrightText: Copyright 2025 Arm Limited and/or its affiliates <open-source-office@arm.com>
// SPDX-License-Identifier: MIT OR Apache-2.0

use core::fmt::{self, Write};
use spin::Lazy;
use spin::Mutex;

use crate::call_ffa;
use crate::CallFfa;
use crate::Error;
use crate::{
    ConsoleLogChars, Interface, Version, CONSOLE_LOG_32_MAX_CHAR_CNT, CONSOLE_LOG_64_MAX_CHAR_CNT,
};

const BUFFER_SIZE: usize = 512;

/// A line-buffered console
/// Console must be initialized before using the `print!` or `println!` macros
///
/// # Example
/// ```
/// #[macro_use]
/// use arm_ffa::{console::initialize, Version};
///
/// initialize(Version(1, 1)).unwrap();
/// println!("Hello, world!");
/// ```
pub struct Console<'ffa> {
    version: Version,
    call_ffa: &'ffa dyn CallFfa,
    buffer_idx: usize,
    buffer: [u8; BUFFER_SIZE],
}

impl<'ffa> Console<'ffa> {
    fn new(version: Version, call_ffa: &'ffa dyn CallFfa) -> Self {
        Self {
            version,
            call_ffa,
            buffer_idx: 0,
            buffer: [0u8; BUFFER_SIZE],
        }
    }
}

// Global, thread-safe, lazily initialized Console
static CONSOLE: Lazy<Mutex<Option<Console<'static>>>> = Lazy::new(|| Mutex::new(None));

struct CallFfaDefault;
impl CallFfa for CallFfaDefault {
    fn call_ffa(&self, version: Version, interface: &Interface) -> Result<(), Error> {
        call_ffa(version, interface)?;
        Ok(())
    }
}

/// Called on program startup to initialize the console.
pub fn initialize(version: Version) -> Result<(), Error> {
    initialize_with_call_ffa(version, &CallFfaDefault)
}

/// Initialize the console with a custom `CallFfa` implementation, to capture underlying
/// ffa calls made.
pub fn initialize_with_call_ffa(
    version: Version,
    call_ffa: &'static dyn CallFfa,
) -> Result<(), Error> {
    let mut console = CONSOLE.lock();
    if console.is_none() {
        *console = Some(Console::new(version, call_ffa));
        Ok(())
    } else {
        Err(Error::ConsoleAlreadyInitialized)
    }
}

/// Get a mutable reference to the global console instance
/// Will panic if console is not initialized
pub fn with_console<F, R>(f: F) -> R
where
    F: FnOnce(&mut Console) -> R,
{
    let mut console = CONSOLE.lock();
    let console_ref = console.as_mut().expect("Console not initialized");
    f(console_ref)
}

impl Write for Console<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for &byte in s.as_bytes() {
            // Flush if buffer is full
            if self.buffer_idx >= BUFFER_SIZE {
                self.flush()?;
            }

            // Write byte to buffer
            self.buffer[self.buffer_idx] = byte;
            self.buffer_idx += 1;

            // Flush if we hit a newline
            if byte == b'\n' {
                self.flush()?;
            }
        }

        Ok(())
    }
}

impl Console<'_> {
    fn flush(&mut self) -> fmt::Result {
        if self.buffer_idx == 0 {
            return Ok(());
        }

        let mut rest = &self.buffer[..self.buffer_idx];
        self.buffer_idx = 0;

        while !rest.is_empty() {
            let (char_cnt, char_lists) = if rest.len() <= CONSOLE_LOG_32_MAX_CHAR_CNT as usize {
                let mut char_lists = [0u32; 6];
                let char_cnt = rest.len();

                rest.chunks(4).enumerate().for_each(|(i, chunk)| {
                    char_lists[i] = encode_chunk_u32(chunk);
                });

                rest = &rest[char_cnt..];
                (char_cnt, ConsoleLogChars::Reg32(char_lists))
            } else {
                let char_cnt = rest.len().min(CONSOLE_LOG_64_MAX_CHAR_CNT as usize);
                let mut char_lists = [0u64; 16];

                rest[..char_cnt]
                    .chunks(8)
                    .enumerate()
                    .for_each(|(i, chunk)| {
                        char_lists[i] = encode_chunk_u64(chunk);
                    });

                rest = &rest[char_cnt..];
                (char_cnt, ConsoleLogChars::Reg64(char_lists))
            };

            let interface = Interface::ConsoleLog {
                char_cnt: char_cnt as u8,
                char_lists,
            };

            self.call_ffa
                .call_ffa(self.version, &interface)
                .map_err(|_| fmt::Error)?;
        }

        Ok(())
    }
}

/// Encode a chunk of data into a u32, padding as necessary
fn encode_chunk_u32(chunk: &[u8]) -> u32 {
    let mut buf = [0u8; 4];
    let len = chunk.len().min(4);
    buf[..len].copy_from_slice(&chunk[..len]);
    u32::from_le_bytes(buf)
}

/// Encode a chunk of data into a u64, padding as necessary
fn encode_chunk_u64(chunk: &[u8]) -> u64 {
    let mut buf = [0u8; 8];
    let len = chunk.len().min(8);
    buf[..len].copy_from_slice(&chunk[..len]);
    u64::from_le_bytes(buf)
}

/// Print formatted data to the console
///
/// # Example
/// ```
/// println!("Hello, {}!", "world");
/// ```
#[cfg(target_arch = "aarch64")]
#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n")
    };
    ($($arg:tt)*) => {
        $crate::print!("{}\n", format_args!($($arg)*))
    };
}

/// Print formatted data to the console without a newline
///
/// # Example
/// ```
/// print!("Hello, {}!", "world");
/// ```
#[cfg(target_arch = "aarch64")]
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        {
            use core::fmt::Write;
            $crate::console::with_console(|console| {
                console.write_fmt(format_args!($($arg)*)).unwrap()
            });
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::RwLock;
    use std::vec::Vec;

    #[derive(Debug, Default)]
    struct CallFfaTest(RwLock<Vec<Interface>>);
    impl CallFfa for CallFfaTest {
        fn call_ffa(&self, _version: Version, interface: &Interface) -> Result<(), Error> {
            self.0.write().unwrap().push(interface.clone());
            Ok(())
        }
    }

    #[test]
    fn test_console_initialization() {
        let version = Version(1, 1);
        static CALL_FFA_TEST: CallFfaTest = CallFfaTest(RwLock::new(Vec::new()));

        // Test successful initialization
        assert!(initialize_with_call_ffa(version, &CALL_FFA_TEST).is_ok());

        // Test double initialization
        assert!(matches!(
            initialize_with_call_ffa(version, &CALL_FFA_TEST),
            Err(Error::ConsoleAlreadyInitialized)
        ));
    }

    #[test]
    fn test_console_32bit_logging() {
        let version = Version(1, 1);
        static CALL_FFA_TEST: CallFfaTest = CallFfaTest(RwLock::new(Vec::new()));
        let mut console = Console::new(version, &CALL_FFA_TEST);

        // Initialize console
        // Write a short message that should use 32-bit logging
        console.write_str("Hello\n").unwrap();

        let calls = CALL_FFA_TEST.0.read().unwrap().clone();
        assert_eq!(calls.len(), 1);

        let interface = &calls[0];

        if let Interface::ConsoleLog {
            char_cnt,
            char_lists,
        } = interface
        {
            assert_eq!(char_cnt, &6);
            if let ConsoleLogChars::Reg32(regs) = char_lists {
                assert_eq!(regs[0], u32::from_le_bytes(*b"Hell"));
                assert_eq!(regs[1], u32::from_le_bytes(*b"o\n\0\0"));
                assert_eq!(regs[2], 0);
                assert_eq!(regs[3], 0);
                assert_eq!(regs[4], 0);
                assert_eq!(regs[5], 0);
            } else {
                panic!("Expected 32-bit console log");
            }
        } else {
            panic!("Expected console log interface");
        }
    }

    #[test]
    fn test_console_64bit_logging() {
        let version = Version(1, 2);
        static CALL_FFA_TEST: CallFfaTest = CallFfaTest(RwLock::new(Vec::new()));
        let mut console = Console::new(version, &CALL_FFA_TEST);

        // Write a long message that should use 64-bit logging
        let long_msg = "A".repeat(CONSOLE_LOG_64_MAX_CHAR_CNT as usize + 1);
        console.write_str(&long_msg).unwrap();
        console.flush().unwrap();

        let calls = CALL_FFA_TEST.0.read().unwrap().clone();
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls[0],
            Interface::ConsoleLog {
                char_cnt: 128,
                char_lists: ConsoleLogChars::Reg64([0x4141414141414141; 16]),
            }
        );
        assert_eq!(
            calls[1],
            Interface::ConsoleLog {
                char_cnt: 1,
                char_lists: ConsoleLogChars::Reg32([0x00000041, 0, 0, 0, 0, 0]),
            }
        );
    }

    #[test]
    fn test_console_buffer_flushing() {
        let version = Version(1, 1);
        static CALL_FFA_TEST: CallFfaTest = CallFfaTest(RwLock::new(Vec::new()));
        let mut console = Console::new(version, &CALL_FFA_TEST);

        // Each newline should flush the buffer, triggering an ffa call
        console.write_str("Hello\nWorld\n").unwrap();

        let calls = CALL_FFA_TEST.0.read().unwrap().clone();
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls[0],
            Interface::ConsoleLog {
                char_cnt: 6,
                char_lists: ConsoleLogChars::Reg32([
                    u32::from_le_bytes(*b"Hell"),
                    u32::from_le_bytes(*b"o\n\0\0"),
                    0,
                    0,
                    0,
                    0,
                ]),
            }
        );
        assert_eq!(
            calls[1],
            Interface::ConsoleLog {
                char_cnt: 6,
                char_lists: ConsoleLogChars::Reg32([
                    u32::from_le_bytes(*b"Worl"),
                    u32::from_le_bytes(*b"d\n\0\0"),
                    0,
                    0,
                    0,
                    0,
                ]),
            }
        );
    }
}
