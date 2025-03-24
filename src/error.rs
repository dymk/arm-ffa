// SPDX-FileCopyrightText: Copyright 2025 Arm Limited and/or its affiliates <open-source-office@arm.com>
// SPDX-License-Identifier: MIT OR Apache-2.0

use num_enum::{IntoPrimitive, TryFromPrimitive};
use thiserror::Error;

/// Rich error types returned by this module. Should be converted to [`crate::FfaError`] when used
/// with the `FFA_ERROR` interface.
#[derive(Debug, Error)]
pub enum Error {
    #[error("Unrecognised FF-A function ID {0}")]
    UnrecognisedFunctionId(u32),
    #[error("Unrecognised FF-A feature ID {0}")]
    UnrecognisedFeatureId(u8),
    #[error("Unrecognised FF-A error code {0}")]
    UnrecognisedErrorCode(i32),
    #[error("Unrecognised FF-A Framework Message {0}")]
    UnrecognisedFwkMsg(u32),
    #[error("Unrecognised FF-A Msg Wait Flag {0}")]
    UnrecognisedMsgWaitFlag(u32),
    #[error("Unrecognised VM availability status {0}")]
    UnrecognisedVmAvailabilityStatus(i32),
    #[error("Unrecognised FF-A Warm Boot Type {0}")]
    UnrecognisedWarmBootType(u32),
    #[error("Invalid version {0}")]
    InvalidVersion(u32),
    #[error("Console already initialized")]
    ConsoleAlreadyInitialized,
}

impl TryFrom<Error> for FfaError {
    type Error = ();

    fn try_from(value: Error) -> Result<Self, Self::Error> {
        match value {
            Error::UnrecognisedFunctionId(_) | Error::UnrecognisedFeatureId(_) => {
                Ok(Self::NotSupported)
            }
            Error::UnrecognisedErrorCode(_)
            | Error::UnrecognisedFwkMsg(_)
            | Error::InvalidVersion(_)
            | Error::UnrecognisedMsgWaitFlag(_)
            | Error::UnrecognisedVmAvailabilityStatus(_)
            | Error::UnrecognisedWarmBootType(_) => Ok(Self::InvalidParameters),
            _ => Err(()),
        }
    }
}

/// Error status codes used by the `FFA_ERROR` interface.
#[derive(Clone, Copy, Debug, Eq, Error, IntoPrimitive, PartialEq, TryFromPrimitive)]
#[num_enum(error_type(name = Error, constructor = Error::UnrecognisedErrorCode))]
#[repr(i32)]
pub enum FfaError {
    #[error("Not supported")]
    NotSupported = -1,
    #[error("Invalid parameters")]
    InvalidParameters = -2,
    #[error("No memory")]
    NoMemory = -3,
    #[error("Busy")]
    Busy = -4,
    #[error("Interrupted")]
    Interrupted = -5,
    #[error("Denied")]
    Denied = -6,
    #[error("Retry")]
    Retry = -7,
    #[error("Aborted")]
    Aborted = -8,
    #[error("No data")]
    NoData = -9,
}
