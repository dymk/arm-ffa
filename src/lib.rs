// SPDX-FileCopyrightText: Copyright 2023 Arm Limited and/or its affiliates <open-source-office@arm.com>
// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg_attr(not(test), no_std)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(unsafe_op_in_unsafe_fn)]
#![doc = include_str!("../README.md")]

pub use call_ffa::{call_ffa, CallFfa};
use core::fmt::{self, Debug, Display, Formatter};
use error::{Error, FfaError};
use func_id::FuncId;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use thiserror::Error;
use uuid::Uuid;

pub mod boot_info;
#[macro_use]
pub mod console;
mod call_ffa;
pub mod error;
mod ffa_v1_1;
mod ffa_v1_2;
pub mod func_id;
pub mod memory_management;
pub mod partition_info;

/// Constant for 4K page size. On many occasions the FF-A spec defines memory size as count of 4K
/// pages, regardless of the current translation granule.
pub const FFA_PAGE_SIZE_4K: usize = 4096;

/// An FF-A instance is a valid combination of two FF-A components at an exception level boundary.
#[derive(PartialEq, Clone, Copy)]
pub enum Instance {
    /// The instance between the SPMC and SPMD.
    SecurePhysical,
    /// The instance between the SPMC and a physical SP (contains the SP's endpoint ID).
    SecureVirtual(u16),
}

/// Endpoint ID and vCPU ID pair, used by `FFA_ERROR`, `FFA_INTERRUPT` and `FFA_RUN` interfaces.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct TargetInfo {
    pub endpoint_id: u16,
    pub vcpu_id: u16,
}

impl From<u32> for TargetInfo {
    fn from(value: u32) -> Self {
        Self {
            endpoint_id: (value >> 16) as u16,
            vcpu_id: value as u16,
        }
    }
}

impl From<TargetInfo> for u32 {
    fn from(value: TargetInfo) -> Self {
        ((value.endpoint_id as u32) << 16) | value.vcpu_id as u32
    }
}

/// Arguments for the `FFA_SUCCESS` interface.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum SuccessArgs {
    Result32([u32; 6]),
    Result64([u64; 6]),
    Result64_2([u64; 16]),
}

/// Entrypoint address argument for `FFA_SECONDARY_EP_REGISTER` interface.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum SecondaryEpRegisterAddr {
    Addr32(u32),
    Addr64(u64),
}

/// Version number of the FF-A implementation, `.0` is the major, `.1` is minor the version.
#[derive(Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
pub struct Version(pub u16, pub u16);

impl Version {
    // The FF-A spec mandates that bit[31] of a version number must be 0
    const MBZ_BITS: u32 = 1 << 31;

    /// Returns whether the caller's version (self) is compatible with the callee's version (input
    /// parameter)
    pub fn is_compatible_to(&self, callee_version: &Version) -> bool {
        self.0 == callee_version.0 && self.1 <= callee_version.1
    }
}

impl TryFrom<u32> for Version {
    type Error = Error;

    fn try_from(val: u32) -> Result<Self, Self::Error> {
        if (val & Self::MBZ_BITS) != 0 {
            Err(Error::InvalidVersion(val))
        } else {
            Ok(Self((val >> 16) as u16, val as u16))
        }
    }
}

impl From<Version> for u32 {
    fn from(v: Version) -> Self {
        let v_u32 = ((v.0 as u32) << 16) | v.1 as u32;
        assert!(v_u32 & Version::MBZ_BITS == 0);
        v_u32
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}.{}", self.0, self.1)
    }
}

impl Debug for Version {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(self, f)
    }
}

/// Feature IDs used by the `FFA_FEATURES` interface.
#[derive(Clone, Copy, Debug, Eq, IntoPrimitive, PartialEq, TryFromPrimitive)]
#[num_enum(error_type(name = Error, constructor = Error::UnrecognisedFeatureId))]
#[repr(u8)]
pub enum FeatureId {
    NotificationPendingInterrupt = 0x1,
    ScheduleReceiverInterrupt = 0x2,
    ManagedExitInterrupt = 0x3,
}

/// Arguments for the `FFA_FEATURES` interface.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum Feature {
    FuncId(FuncId),
    FeatureId(FeatureId),
    Unknown(u32),
}

impl From<u32> for Feature {
    fn from(value: u32) -> Self {
        // Bit[31] is set for all valid FF-A function IDs so we don't have to check it separately
        if let Ok(func_id) = value.try_into() {
            Self::FuncId(func_id)
        } else if let Ok(feat_id) = (value as u8).try_into() {
            Self::FeatureId(feat_id)
        } else {
            Self::Unknown(value)
        }
    }
}

impl From<Feature> for u32 {
    fn from(value: Feature) -> Self {
        match value {
            Feature::FuncId(func_id) => (1 << 31) | func_id as u32,
            Feature::FeatureId(feature_id) => feature_id as u32,
            Feature::Unknown(id) => panic!("Unknown feature or function ID {:#x?}", id),
        }
    }
}

/// RXTX buffer descriptor, used by `FFA_RXTX_MAP`.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum RxTxAddr {
    Addr32 { rx: u32, tx: u32 },
    Addr64 { rx: u64, tx: u64 },
}

/// Composite type for capturing success and error return codes for the VM availability messages.
///
/// Error codes are handled by the `FfaError` type. Having a separate type for errors helps using
/// `Result<(), FfaError>`. If a single type would include both success and error values,
/// then `Err(FfaError::Success)` would be incomprehensible.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum VmAvailabilityStatus {
    Success,
    Error(FfaError),
}

impl TryFrom<i32> for VmAvailabilityStatus {
    type Error = Error;
    fn try_from(value: i32) -> Result<Self, <Self as TryFrom<i32>>::Error> {
        Ok(match value {
            0 => Self::Success,
            error_code => Self::Error(FfaError::try_from(error_code)?),
        })
    }
}

impl From<VmAvailabilityStatus> for i32 {
    fn from(value: VmAvailabilityStatus) -> Self {
        match value {
            VmAvailabilityStatus::Success => 0,
            VmAvailabilityStatus::Error(error_code) => error_code.into(),
        }
    }
}

/// Arguments for the Power Warm Boot `FFA_MSG_SEND_DIRECT_REQ` interface.
#[derive(Clone, Copy, Debug, Eq, IntoPrimitive, PartialEq, TryFromPrimitive)]
#[num_enum(error_type(name = Error, constructor = Error::UnrecognisedWarmBootType))]
#[repr(u32)]
pub enum WarmBootType {
    ExitFromSuspend = 0,
    ExitFromLowPower = 1,
}

/// Arguments for the `FFA_MSG_SEND_DIRECT_{REQ,RESP}` interfaces.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum DirectMsgArgs {
    Args32([u32; 5]),
    Args64([u64; 5]),
    /// Message for forwarding FFA_VERSION call from Normal world to the SPMC
    VersionReq {
        version: Version,
    },
    /// Response message to forwarded FFA_VERSION call from the Normal world
    /// Contains the version returned by the SPMC or None
    VersionResp {
        version: Option<Version>,
    },
    /// Message for a power management operation initiated by a PSCI function
    PowerPsciReq32 {
        function_id: u32,
        // params[i]: Input parameter in w[i] in PSCI function invocation at EL3.
        params: [u32; 3],
    },
    /// Message for a power management operation initiated by a PSCI function
    PowerPsciReq64 {
        function_id: u32,
        // params[i]: Input parameter in x[i] in PSCI function invocation at EL3.
        params: [u64; 3],
    },
    /// Message for a warm boot
    PowerWarmBootReq {
        boot_type: WarmBootType,
    },
    /// Response message to indicate return status of the last power management request message
    /// Return error code SUCCESS or DENIED as defined in PSCI spec. Caller is left to do the
    /// parsing of the return status.
    PowerPsciResp {
        // TODO: Use arm-psci crate's return status here.
        psci_status: i32,
    },
    /// Message to signal creation of a VM
    VmCreated {
        // Globally unique Handle to identify a memory region that contains IMPLEMENTATION DEFINED
        // information associated with the created VM.
        // The invalid memory region handle must be specified by the Hypervisor if this field is not
        //  used.
        handle: memory_management::Handle,
        vm_id: u16,
    },
    /// Message to acknowledge creation of a VM
    VmCreatedAck {
        sp_status: VmAvailabilityStatus,
    },
    /// Message to signal destruction of a VM
    VmDestructed {
        // Globally unique Handle to identify a memory region that contains IMPLEMENTATION DEFINED
        // information associated with the created VM.
        // The invalid memory region handle must be specified by the Hypervisor if this field is not
        //  used.
        handle: memory_management::Handle,
        vm_id: u16,
    },
    /// Message to acknowledge destruction of a VM
    VmDestructedAck {
        sp_status: VmAvailabilityStatus,
    },
}

impl DirectMsgArgs {
    // Flags for the `FFA_MSG_SEND_DIRECT_{REQ,RESP}` interfaces.

    const FWK_MSG_BITS: u32 = 1 << 31;
    const VERSION_REQ: u32 = DirectMsgArgs::FWK_MSG_BITS | 0b1000;
    const VERSION_RESP: u32 = DirectMsgArgs::FWK_MSG_BITS | 0b1001;
    const POWER_PSCI_REQ: u32 = DirectMsgArgs::FWK_MSG_BITS;
    const POWER_WARM_BOOT_REQ: u32 = DirectMsgArgs::FWK_MSG_BITS | 0b0001;
    const POWER_PSCI_RESP: u32 = DirectMsgArgs::FWK_MSG_BITS | 0b0010;
    const VM_CREATED: u32 = DirectMsgArgs::FWK_MSG_BITS | 0b0100;
    const VM_CREATED_ACK: u32 = DirectMsgArgs::FWK_MSG_BITS | 0b0101;
    const VM_DESTRUCTED: u32 = DirectMsgArgs::FWK_MSG_BITS | 0b0110;
    const VM_DESTRUCTED_ACK: u32 = DirectMsgArgs::FWK_MSG_BITS | 0b0111;
}

/// Arguments for the `FFA_MSG_SEND_DIRECT_{REQ,RESP}2` interfaces.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct DirectMsg2Args([u64; 14]);

/// Descriptor for a dynamically allocated memory buffer that contains the memory transaction
/// descriptor.
///
/// Used by `FFA_MEM_{DONATE,LEND,SHARE,RETRIEVE_REQ}` interfaces, only when the TX buffer is not
/// used to transmit the transaction descriptor.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum MemOpBuf {
    Buf32 { addr: u32, page_cnt: u32 },
    Buf64 { addr: u64, page_cnt: u32 },
}

/// Memory address argument for `FFA_MEM_PERM_{GET,SET}` interfaces.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum MemAddr {
    Addr32(u32),
    Addr64(u64),
}

/// Argument for the `FFA_CONSOLE_LOG` interface.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum ConsoleLogChars {
    Reg32([u32; 6]),
    Reg64([u64; 16]),
}

/// FF-A "message types", the terminology used by the spec is "interfaces".
///
/// The interfaces are used by FF-A components for communication at an FF-A instance. The spec also
/// describes the valid FF-A instances and conduits for each interface.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum Interface {
    Error {
        target_info: TargetInfo,
        error_code: FfaError,
    },
    Success {
        target_info: u32,
        args: SuccessArgs,
    },
    Interrupt {
        target_info: TargetInfo,
        interrupt_id: u32,
    },
    Version {
        input_version: Version,
    },
    VersionOut {
        output_version: Version,
    },
    Features {
        feat_id: Feature,
        input_properties: u32,
    },
    RxAcquire {
        vm_id: u16,
    },
    RxRelease {
        vm_id: u16,
    },
    RxTxMap {
        addr: RxTxAddr,
        page_cnt: u32,
    },
    RxTxUnmap {
        id: u16,
    },
    PartitionInfoGet {
        uuid: Uuid,
        flags: u32,
    },
    IdGet,
    SpmIdGet,
    MsgWait,
    Yield,
    Run {
        target_info: TargetInfo,
    },
    NormalWorldResume,
    SecondaryEpRegister {
        entrypoint: SecondaryEpRegisterAddr,
    },
    MsgSend2 {
        sender_vm_id: u16,
        flags: u32,
    },
    MsgSendDirectReq {
        src_id: u16,
        dst_id: u16,
        args: DirectMsgArgs,
    },
    MsgSendDirectResp {
        src_id: u16,
        dst_id: u16,
        args: DirectMsgArgs,
    },
    MsgSendDirectReq2 {
        src_id: u16,
        dst_id: u16,
        uuid: Uuid,
        args: DirectMsg2Args,
    },
    MsgSendDirectResp2 {
        src_id: u16,
        dst_id: u16,
        args: DirectMsg2Args,
    },
    MemDonate {
        total_len: u32,
        frag_len: u32,
        buf: Option<MemOpBuf>,
    },
    MemLend {
        total_len: u32,
        frag_len: u32,
        buf: Option<MemOpBuf>,
    },
    MemShare {
        total_len: u32,
        frag_len: u32,
        buf: Option<MemOpBuf>,
    },
    MemRetrieveReq {
        total_len: u32,
        frag_len: u32,
        buf: Option<MemOpBuf>,
    },
    MemRetrieveResp {
        total_len: u32,
        frag_len: u32,
    },
    MemRelinquish,
    MemReclaim {
        handle: memory_management::Handle,
        flags: u32,
    },
    MemPermGet {
        addr: MemAddr,
        page_cnt: Option<u32>,
    },
    MemPermSet {
        addr: MemAddr,
        page_cnt: u32,
        mem_perm: u32,
    },
    ConsoleLog {
        char_cnt: u8,
        char_lists: ConsoleLogChars,
    },
}

impl Interface {
    /// Returns the function ID for the call, if it has one.
    pub fn function_id(&self) -> Option<FuncId> {
        match self {
            Interface::Error { .. } => Some(FuncId::Error),
            Interface::Success { args, .. } => match args {
                SuccessArgs::Result32(..) => Some(FuncId::Success32),
                SuccessArgs::Result64(..) | SuccessArgs::Result64_2(..) => Some(FuncId::Success64),
            },
            Interface::Interrupt { .. } => Some(FuncId::Interrupt),
            Interface::Version { .. } => Some(FuncId::Version),
            Interface::VersionOut { .. } => None,
            Interface::Features { .. } => Some(FuncId::Features),
            Interface::RxAcquire { .. } => Some(FuncId::RxAcquire),
            Interface::RxRelease { .. } => Some(FuncId::RxRelease),
            Interface::RxTxMap { addr, .. } => match addr {
                RxTxAddr::Addr32 { .. } => Some(FuncId::RxTxMap32),
                RxTxAddr::Addr64 { .. } => Some(FuncId::RxTxMap64),
            },
            Interface::RxTxUnmap { .. } => Some(FuncId::RxTxUnmap),
            Interface::PartitionInfoGet { .. } => Some(FuncId::PartitionInfoGet),
            Interface::IdGet => Some(FuncId::IdGet),
            Interface::SpmIdGet => Some(FuncId::SpmIdGet),
            Interface::MsgWait => Some(FuncId::MsgWait),
            Interface::Yield => Some(FuncId::Yield),
            Interface::Run { .. } => Some(FuncId::Run),
            Interface::NormalWorldResume => Some(FuncId::NormalWorldResume),
            Interface::SecondaryEpRegister { entrypoint } => match entrypoint {
                SecondaryEpRegisterAddr::Addr32 { .. } => Some(FuncId::SecondaryEpRegister32),
                SecondaryEpRegisterAddr::Addr64 { .. } => Some(FuncId::SecondaryEpRegister64),
            },
            Interface::MsgSend2 { .. } => Some(FuncId::MsgSend2),
            Interface::MsgSendDirectReq { args, .. } => match args {
                DirectMsgArgs::Args32(_) => Some(FuncId::MsgSendDirectReq32),
                DirectMsgArgs::Args64(_) => Some(FuncId::MsgSendDirectReq64),
                DirectMsgArgs::VersionReq { .. } => Some(FuncId::MsgSendDirectReq32),
                DirectMsgArgs::PowerPsciReq32 { .. } => Some(FuncId::MsgSendDirectReq32),
                DirectMsgArgs::PowerPsciReq64 { .. } => Some(FuncId::MsgSendDirectReq64),
                DirectMsgArgs::PowerWarmBootReq { .. } => Some(FuncId::MsgSendDirectReq32),
                DirectMsgArgs::VmCreated { .. } => Some(FuncId::MsgSendDirectReq32),
                DirectMsgArgs::VmDestructed { .. } => Some(FuncId::MsgSendDirectReq32),
                _ => None,
            },
            Interface::MsgSendDirectResp { args, .. } => match args {
                DirectMsgArgs::Args32(_) => Some(FuncId::MsgSendDirectResp32),
                DirectMsgArgs::Args64(_) => Some(FuncId::MsgSendDirectResp64),
                DirectMsgArgs::VersionResp { .. } => Some(FuncId::MsgSendDirectResp32),
                DirectMsgArgs::PowerPsciResp { .. } => Some(FuncId::MsgSendDirectResp32),
                DirectMsgArgs::VmCreatedAck { .. } => Some(FuncId::MsgSendDirectResp32),
                DirectMsgArgs::VmDestructedAck { .. } => Some(FuncId::MsgSendDirectResp32),
                _ => None,
            },
            Interface::MsgSendDirectReq2 { .. } => Some(FuncId::MsgSendDirectReq64_2),
            Interface::MsgSendDirectResp2 { .. } => Some(FuncId::MsgSendDirectResp64_2),
            Interface::MemDonate { buf, .. } => match buf {
                Some(MemOpBuf::Buf64 { .. }) => Some(FuncId::MemDonate64),
                _ => Some(FuncId::MemDonate32),
            },
            Interface::MemLend { buf, .. } => match buf {
                Some(MemOpBuf::Buf64 { .. }) => Some(FuncId::MemLend64),
                _ => Some(FuncId::MemLend32),
            },
            Interface::MemShare { buf, .. } => match buf {
                Some(MemOpBuf::Buf64 { .. }) => Some(FuncId::MemShare64),
                _ => Some(FuncId::MemShare32),
            },
            Interface::MemRetrieveReq { buf, .. } => match buf {
                Some(MemOpBuf::Buf64 { .. }) => Some(FuncId::MemRetrieveReq64),
                _ => Some(FuncId::MemRetrieveReq32),
            },
            Interface::MemRetrieveResp { .. } => Some(FuncId::MemRetrieveResp),
            Interface::MemRelinquish => Some(FuncId::MemRelinquish),
            Interface::MemReclaim { .. } => Some(FuncId::MemReclaim),
            Interface::MemPermGet { addr, .. } => match addr {
                MemAddr::Addr32(_) => Some(FuncId::MemPermGet32),
                MemAddr::Addr64(_) => Some(FuncId::MemPermGet64),
            },
            Interface::MemPermSet { addr, .. } => match addr {
                MemAddr::Addr32(_) => Some(FuncId::MemPermSet32),
                MemAddr::Addr64(_) => Some(FuncId::MemPermSet64),
            },
            Interface::ConsoleLog { char_lists, .. } => match char_lists {
                ConsoleLogChars::Reg32(_) => Some(FuncId::ConsoleLog32),
                ConsoleLogChars::Reg64(_) => Some(FuncId::ConsoleLog64),
            },
        }
    }

    /// Returns true if this is a 32-bit call, or false if it is a 64-bit call.
    pub fn is_32bit(&self) -> bool {
        // TODO: self should always have a function ID?
        self.function_id().unwrap().is_32bit()
    }

    /// Parse interface from register contents. The caller must ensure that the `regs` argument has
    /// the correct length: 8 registers for FF-A v1.1 and lower, 18 registers for v1.2 and higher.
    pub fn from_regs(version: Version, regs: &[u64]) -> Result<Self, Error> {
        let fid = FuncId::try_from(regs[0] as u32)?;

        let msg = match fid {
            FuncId::Error => Self::Error {
                target_info: (regs[1] as u32).into(),
                error_code: FfaError::try_from(regs[2] as i32)?,
            },
            FuncId::Success32 => Self::Success {
                target_info: regs[1] as u32,
                args: SuccessArgs::Result32([
                    regs[2] as u32,
                    regs[3] as u32,
                    regs[4] as u32,
                    regs[5] as u32,
                    regs[6] as u32,
                    regs[7] as u32,
                ]),
            },
            FuncId::Success64 if version < Version(1, 2) => Self::Success {
                target_info: regs[1] as u32,
                args: SuccessArgs::Result64([regs[2], regs[3], regs[4], regs[5], regs[6], regs[7]]),
            },
            FuncId::Interrupt => Self::Interrupt {
                target_info: (regs[1] as u32).into(),
                interrupt_id: regs[2] as u32,
            },
            FuncId::Version => Self::Version {
                input_version: (regs[1] as u32).try_into()?,
            },
            FuncId::Features => Self::Features {
                feat_id: (regs[1] as u32).into(),
                input_properties: regs[2] as u32,
            },
            FuncId::RxAcquire => Self::RxAcquire {
                vm_id: regs[1] as u16,
            },
            FuncId::RxRelease => Self::RxRelease {
                vm_id: regs[1] as u16,
            },
            FuncId::RxTxMap32 => {
                let addr = RxTxAddr::Addr32 {
                    rx: regs[2] as u32,
                    tx: regs[1] as u32,
                };
                let page_cnt = regs[3] as u32;

                Self::RxTxMap { addr, page_cnt }
            }
            FuncId::RxTxMap64 => {
                let addr = RxTxAddr::Addr64 {
                    rx: regs[2],
                    tx: regs[1],
                };
                let page_cnt = regs[3] as u32;

                Self::RxTxMap { addr, page_cnt }
            }
            FuncId::RxTxUnmap => Self::RxTxUnmap { id: regs[1] as u16 },
            FuncId::PartitionInfoGet => {
                let uuid_words = [
                    regs[1] as u32,
                    regs[2] as u32,
                    regs[3] as u32,
                    regs[4] as u32,
                ];
                let mut bytes: [u8; 16] = [0; 16];
                for (i, b) in uuid_words.iter().flat_map(|w| w.to_le_bytes()).enumerate() {
                    bytes[i] = b;
                }
                Self::PartitionInfoGet {
                    uuid: Uuid::from_bytes(bytes),
                    flags: regs[5] as u32,
                }
            }
            FuncId::IdGet => Self::IdGet,
            FuncId::SpmIdGet => Self::SpmIdGet,
            FuncId::MsgWait => Self::MsgWait,
            FuncId::Yield => Self::Yield,
            FuncId::Run => Self::Run {
                target_info: (regs[1] as u32).into(),
            },
            FuncId::NormalWorldResume => Self::NormalWorldResume,
            FuncId::SecondaryEpRegister32 => Self::SecondaryEpRegister {
                entrypoint: SecondaryEpRegisterAddr::Addr32(regs[1] as u32),
            },
            FuncId::SecondaryEpRegister64 => Self::SecondaryEpRegister {
                entrypoint: SecondaryEpRegisterAddr::Addr64(regs[1]),
            },
            FuncId::MsgSend2 => Self::MsgSend2 {
                sender_vm_id: regs[1] as u16,
                flags: regs[2] as u32,
            },
            FuncId::MsgSendDirectReq32 => Self::MsgSendDirectReq {
                src_id: (regs[1] >> 16) as u16,
                dst_id: regs[1] as u16,
                args: if (regs[2] as u32 & DirectMsgArgs::FWK_MSG_BITS) != 0 {
                    match regs[2] as u32 {
                        DirectMsgArgs::VERSION_REQ => DirectMsgArgs::VersionReq {
                            version: Version::try_from(regs[3] as u32)?,
                        },
                        DirectMsgArgs::POWER_PSCI_REQ => DirectMsgArgs::PowerPsciReq32 {
                            function_id: regs[3] as u32,
                            params: [regs[4] as u32, regs[5] as u32, regs[6] as u32],
                        },
                        DirectMsgArgs::POWER_WARM_BOOT_REQ => DirectMsgArgs::PowerWarmBootReq {
                            boot_type: WarmBootType::try_from(regs[3] as u32)?,
                        },
                        DirectMsgArgs::VM_CREATED => DirectMsgArgs::VmCreated {
                            handle: memory_management::Handle::from([
                                regs[3] as u32,
                                regs[4] as u32,
                            ]),
                            vm_id: regs[5] as u16,
                        },
                        DirectMsgArgs::VM_DESTRUCTED => DirectMsgArgs::VmDestructed {
                            handle: memory_management::Handle::from([
                                regs[3] as u32,
                                regs[4] as u32,
                            ]),
                            vm_id: regs[5] as u16,
                        },
                        _ => return Err(Error::UnrecognisedFwkMsg(regs[2] as u32)),
                    }
                } else {
                    DirectMsgArgs::Args32([
                        regs[3] as u32,
                        regs[4] as u32,
                        regs[5] as u32,
                        regs[6] as u32,
                        regs[7] as u32,
                    ])
                },
            },
            FuncId::MsgSendDirectReq64 => Self::MsgSendDirectReq {
                src_id: (regs[1] >> 16) as u16,
                dst_id: regs[1] as u16,
                args: if (regs[2] & DirectMsgArgs::FWK_MSG_BITS as u64) != 0 {
                    match regs[2] as u32 {
                        DirectMsgArgs::POWER_PSCI_REQ => DirectMsgArgs::PowerPsciReq64 {
                            function_id: regs[3] as u32,
                            params: [regs[4], regs[5], regs[6]],
                        },
                        _ => return Err(Error::UnrecognisedFwkMsg(regs[2] as u32)),
                    }
                } else {
                    DirectMsgArgs::Args64([regs[3], regs[4], regs[5], regs[6], regs[7]])
                },
            },
            FuncId::MsgSendDirectResp32 => Self::MsgSendDirectResp {
                src_id: (regs[1] >> 16) as u16,
                dst_id: regs[1] as u16,
                args: if (regs[2] as u32 & DirectMsgArgs::FWK_MSG_BITS) != 0 {
                    match regs[2] as u32 {
                        DirectMsgArgs::VERSION_RESP => {
                            if regs[3] as i32 == FfaError::NotSupported.into() {
                                DirectMsgArgs::VersionResp { version: None }
                            } else {
                                DirectMsgArgs::VersionResp {
                                    version: Some(Version::try_from(regs[3] as u32)?),
                                }
                            }
                        }
                        DirectMsgArgs::POWER_PSCI_RESP => DirectMsgArgs::PowerPsciResp {
                            psci_status: regs[3] as i32,
                        },
                        DirectMsgArgs::VM_CREATED_ACK => DirectMsgArgs::VmCreatedAck {
                            sp_status: (regs[3] as i32).try_into()?,
                        },
                        DirectMsgArgs::VM_DESTRUCTED_ACK => DirectMsgArgs::VmDestructedAck {
                            sp_status: (regs[3] as i32).try_into()?,
                        },
                        _ => return Err(Error::UnrecognisedFwkMsg(regs[2] as u32)),
                    }
                } else {
                    DirectMsgArgs::Args32([
                        regs[3] as u32,
                        regs[4] as u32,
                        regs[5] as u32,
                        regs[6] as u32,
                        regs[7] as u32,
                    ])
                },
            },
            FuncId::MsgSendDirectResp64 => Self::MsgSendDirectResp {
                src_id: (regs[1] >> 16) as u16,
                dst_id: regs[1] as u16,
                args: if (regs[2] & DirectMsgArgs::FWK_MSG_BITS as u64) != 0 {
                    return Err(Error::UnrecognisedFwkMsg(regs[2] as u32));
                } else {
                    DirectMsgArgs::Args64([regs[3], regs[4], regs[5], regs[6], regs[7]])
                },
            },
            FuncId::MemDonate32 => Self::MemDonate {
                total_len: regs[1] as u32,
                frag_len: regs[2] as u32,
                buf: if regs[3] != 0 && regs[4] != 0 {
                    Some(MemOpBuf::Buf32 {
                        addr: regs[3] as u32,
                        page_cnt: regs[4] as u32,
                    })
                } else {
                    None
                },
            },
            FuncId::MemDonate64 => Self::MemDonate {
                total_len: regs[1] as u32,
                frag_len: regs[2] as u32,
                buf: if regs[3] != 0 && regs[4] != 0 {
                    Some(MemOpBuf::Buf64 {
                        addr: regs[3],
                        page_cnt: regs[4] as u32,
                    })
                } else {
                    None
                },
            },
            FuncId::MemLend32 => Self::MemLend {
                total_len: regs[1] as u32,
                frag_len: regs[2] as u32,
                buf: if regs[3] != 0 && regs[4] != 0 {
                    Some(MemOpBuf::Buf32 {
                        addr: regs[3] as u32,
                        page_cnt: regs[4] as u32,
                    })
                } else {
                    None
                },
            },
            FuncId::MemLend64 => Self::MemLend {
                total_len: regs[1] as u32,
                frag_len: regs[2] as u32,
                buf: if regs[3] != 0 && regs[4] != 0 {
                    Some(MemOpBuf::Buf64 {
                        addr: regs[3],
                        page_cnt: regs[4] as u32,
                    })
                } else {
                    None
                },
            },
            FuncId::MemShare32 => Self::MemShare {
                total_len: regs[1] as u32,
                frag_len: regs[2] as u32,
                buf: if regs[3] != 0 && regs[4] != 0 {
                    Some(MemOpBuf::Buf32 {
                        addr: regs[3] as u32,
                        page_cnt: regs[4] as u32,
                    })
                } else {
                    None
                },
            },
            FuncId::MemShare64 => Self::MemShare {
                total_len: regs[1] as u32,
                frag_len: regs[2] as u32,
                buf: if regs[3] != 0 && regs[4] != 0 {
                    Some(MemOpBuf::Buf64 {
                        addr: regs[3],
                        page_cnt: regs[4] as u32,
                    })
                } else {
                    None
                },
            },
            FuncId::MemRetrieveReq32 => Self::MemRetrieveReq {
                total_len: regs[1] as u32,
                frag_len: regs[2] as u32,
                buf: if regs[3] != 0 && regs[4] != 0 {
                    Some(MemOpBuf::Buf32 {
                        addr: regs[3] as u32,
                        page_cnt: regs[4] as u32,
                    })
                } else {
                    None
                },
            },
            FuncId::MemRetrieveReq64 => Self::MemRetrieveReq {
                total_len: regs[1] as u32,
                frag_len: regs[2] as u32,
                buf: if regs[3] != 0 && regs[4] != 0 {
                    Some(MemOpBuf::Buf64 {
                        addr: regs[3],
                        page_cnt: regs[4] as u32,
                    })
                } else {
                    None
                },
            },
            FuncId::MemRetrieveResp => Self::MemRetrieveResp {
                total_len: regs[1] as u32,
                frag_len: regs[2] as u32,
            },
            FuncId::MemRelinquish => Self::MemRelinquish,
            FuncId::MemReclaim => Self::MemReclaim {
                handle: memory_management::Handle::from([regs[1] as u32, regs[2] as u32]),
                flags: regs[3] as u32,
            },
            FuncId::MemPermGet32 => Self::MemPermGet {
                addr: MemAddr::Addr32(regs[1] as u32),
                page_cnt: if version >= Version(1, 3) {
                    Some(regs[2] as u32)
                } else {
                    None
                },
            },
            FuncId::MemPermGet64 => Self::MemPermGet {
                addr: MemAddr::Addr64(regs[1]),
                page_cnt: if version >= Version(1, 3) {
                    Some(regs[2] as u32)
                } else {
                    None
                },
            },
            FuncId::MemPermSet32 => Self::MemPermSet {
                addr: MemAddr::Addr32(regs[1] as u32),
                page_cnt: regs[2] as u32,
                mem_perm: regs[3] as u32,
            },
            FuncId::MemPermSet64 => Self::MemPermSet {
                addr: MemAddr::Addr64(regs[1]),
                page_cnt: regs[2] as u32,
                mem_perm: regs[3] as u32,
            },
            FuncId::ConsoleLog32 => Self::ConsoleLog {
                char_cnt: regs[1] as u8,
                char_lists: ConsoleLogChars::Reg32([
                    regs[2] as u32,
                    regs[3] as u32,
                    regs[4] as u32,
                    regs[5] as u32,
                    regs[6] as u32,
                    regs[7] as u32,
                ]),
            },
            FuncId::Success64 if version >= Version(1, 2) => Self::Success {
                target_info: regs[1] as u32,
                args: SuccessArgs::Result64_2(regs[2..18].try_into().unwrap()),
            },
            FuncId::MsgSendDirectReq64_2 => Self::MsgSendDirectReq2 {
                src_id: (regs[1] >> 16) as u16,
                dst_id: regs[1] as u16,
                uuid: Uuid::from_u64_pair(regs[2], regs[3]),
                args: DirectMsg2Args(regs[4..18].try_into().unwrap()),
            },
            FuncId::MsgSendDirectResp64_2 => Self::MsgSendDirectResp2 {
                src_id: (regs[1] >> 16) as u16,
                dst_id: regs[1] as u16,
                args: DirectMsg2Args(regs[4..18].try_into().unwrap()),
            },
            FuncId::ConsoleLog64 => Self::ConsoleLog {
                char_cnt: regs[1] as u8,
                char_lists: ConsoleLogChars::Reg64(regs[2..18].try_into().unwrap()),
            },
            _ => unimplemented!("{:#010x}", fid as u32),
        };

        Ok(msg)
    }

    /// Create register contents for an interface.
    pub fn to_regs(&self, version: Version, a: &mut [u64]) {
        a.fill(0);
        if let Some(function_id) = self.function_id() {
            a[0] = function_id as u64;
        }

        match *self {
            Interface::Error {
                target_info,
                error_code,
            } => {
                a[1] = u32::from(target_info).into();
                a[2] = (error_code as u32).into();
            }
            Interface::Success { target_info, args } if version <= Version(1, 1) => {
                a[1] = target_info.into();
                match args {
                    SuccessArgs::Result32(regs) => {
                        a[2] = regs[0].into();
                        a[3] = regs[1].into();
                        a[4] = regs[2].into();
                        a[5] = regs[3].into();
                        a[6] = regs[4].into();
                        a[7] = regs[5].into();
                    }
                    SuccessArgs::Result64(regs) => {
                        a[2] = regs[0];
                        a[3] = regs[1];
                        a[4] = regs[2];
                        a[5] = regs[3];
                        a[6] = regs[4];
                        a[7] = regs[5];
                    }
                    _ => panic!("{:#x?} requires 18 registers", args),
                }
            }
            Interface::Interrupt {
                target_info,
                interrupt_id,
            } => {
                a[1] = u32::from(target_info).into();
                a[2] = interrupt_id.into();
            }
            Interface::Version { input_version } => {
                a[1] = u32::from(input_version).into();
            }
            Interface::VersionOut { output_version } => {
                a[0] = u32::from(output_version).into();
            }
            Interface::Features {
                feat_id,
                input_properties,
            } => {
                a[1] = u32::from(feat_id).into();
                a[2] = input_properties.into();
            }
            Interface::RxAcquire { vm_id } => {
                a[1] = vm_id.into();
            }
            Interface::RxRelease { vm_id } => {
                a[1] = vm_id.into();
            }
            Interface::RxTxMap { addr, page_cnt } => {
                match addr {
                    RxTxAddr::Addr32 { rx, tx } => {
                        a[1] = tx.into();
                        a[2] = rx.into();
                    }
                    RxTxAddr::Addr64 { rx, tx } => {
                        a[1] = tx;
                        a[2] = rx;
                    }
                }
                a[3] = page_cnt.into();
            }
            Interface::RxTxUnmap { id } => {
                a[1] = id.into();
            }
            Interface::PartitionInfoGet { uuid, flags } => {
                let bytes = uuid.into_bytes();
                a[1] = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]).into();
                a[2] = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]).into();
                a[3] = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]).into();
                a[4] = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]).into();
                a[5] = flags.into();
            }
            Interface::IdGet | Interface::SpmIdGet | Interface::MsgWait | Interface::Yield => {}
            Interface::Run { target_info } => {
                a[1] = u32::from(target_info).into();
            }
            Interface::NormalWorldResume => {}
            Interface::SecondaryEpRegister { entrypoint } => match entrypoint {
                SecondaryEpRegisterAddr::Addr32(addr) => a[1] = addr as u64,
                SecondaryEpRegisterAddr::Addr64(addr) => a[1] = addr,
            },
            Interface::MsgSend2 {
                sender_vm_id,
                flags,
            } => {
                a[1] = sender_vm_id.into();
                a[2] = flags.into();
            }
            Interface::MsgSendDirectReq {
                src_id,
                dst_id,
                args,
            } => {
                a[1] = ((src_id as u64) << 16) | dst_id as u64;
                match args {
                    DirectMsgArgs::Args32(args) => {
                        a[3] = args[0].into();
                        a[4] = args[1].into();
                        a[5] = args[2].into();
                        a[6] = args[3].into();
                        a[7] = args[4].into();
                    }
                    DirectMsgArgs::Args64(args) => {
                        a[3] = args[0];
                        a[4] = args[1];
                        a[5] = args[2];
                        a[6] = args[3];
                        a[7] = args[4];
                    }
                    DirectMsgArgs::VersionReq { version } => {
                        a[2] = DirectMsgArgs::VERSION_REQ.into();
                        a[3] = u32::from(version).into();
                    }
                    DirectMsgArgs::PowerPsciReq32 {
                        function_id,
                        params,
                    } => {
                        a[2] = DirectMsgArgs::POWER_PSCI_REQ.into();
                        a[3] = function_id.into();
                        a[4] = params[0].into();
                        a[5] = params[1].into();
                        a[6] = params[2].into();
                    }
                    DirectMsgArgs::PowerPsciReq64 {
                        function_id,
                        params,
                    } => {
                        a[2] = DirectMsgArgs::POWER_PSCI_REQ.into();
                        a[3] = function_id.into();
                        a[4] = params[0];
                        a[5] = params[1];
                        a[6] = params[2];
                    }
                    DirectMsgArgs::PowerWarmBootReq { boot_type } => {
                        a[2] = DirectMsgArgs::POWER_WARM_BOOT_REQ.into();
                        a[3] = u32::from(boot_type).into();
                    }
                    DirectMsgArgs::VmCreated { handle, vm_id } => {
                        a[2] = DirectMsgArgs::VM_CREATED.into();
                        let handle_regs: [u32; 2] = handle.into();
                        a[3] = handle_regs[0].into();
                        a[4] = handle_regs[1].into();
                        a[5] = vm_id.into();
                    }
                    DirectMsgArgs::VmDestructed { handle, vm_id } => {
                        a[2] = DirectMsgArgs::VM_DESTRUCTED.into();
                        let handle_regs: [u32; 2] = handle.into();
                        a[3] = handle_regs[0].into();
                        a[4] = handle_regs[1].into();
                        a[5] = vm_id.into();
                    }
                    _ => panic!("Malformed MsgSendDirectReq interface"),
                }
            }
            Interface::MsgSendDirectResp {
                src_id,
                dst_id,
                args,
            } => {
                a[1] = ((src_id as u64) << 16) | dst_id as u64;
                match args {
                    DirectMsgArgs::Args32(args) => {
                        a[3] = args[0].into();
                        a[4] = args[1].into();
                        a[5] = args[2].into();
                        a[6] = args[3].into();
                        a[7] = args[4].into();
                    }
                    DirectMsgArgs::Args64(args) => {
                        a[3] = args[0];
                        a[4] = args[1];
                        a[5] = args[2];
                        a[6] = args[3];
                        a[7] = args[4];
                    }
                    DirectMsgArgs::VersionResp { version } => {
                        a[2] = DirectMsgArgs::VERSION_RESP.into();
                        match version {
                            None => a[3] = i32::from(FfaError::NotSupported) as u64,
                            Some(ver) => a[3] = u32::from(ver).into(),
                        }
                    }
                    DirectMsgArgs::PowerPsciResp { psci_status } => {
                        a[2] = DirectMsgArgs::POWER_PSCI_RESP.into();
                        a[3] = psci_status as u64;
                    }
                    DirectMsgArgs::VmCreatedAck { sp_status } => {
                        a[2] = DirectMsgArgs::VM_CREATED_ACK.into();
                        a[4] = i32::from(sp_status) as u64;
                    }
                    DirectMsgArgs::VmDestructedAck { sp_status } => {
                        a[2] = DirectMsgArgs::VM_DESTRUCTED_ACK.into();
                        a[3] = i32::from(sp_status) as u64;
                    }
                    _ => panic!("Malformed MsgSendDirectResp interface"),
                }
            }
            Interface::MemDonate {
                total_len,
                frag_len,
                buf,
            } => {
                a[1] = total_len.into();
                a[2] = frag_len.into();
                (a[3], a[4]) = match buf {
                    Some(MemOpBuf::Buf32 { addr, page_cnt }) => (addr.into(), page_cnt.into()),
                    Some(MemOpBuf::Buf64 { addr, page_cnt }) => (addr, page_cnt.into()),
                    None => (0, 0),
                };
            }
            Interface::MemLend {
                total_len,
                frag_len,
                buf,
            } => {
                a[1] = total_len.into();
                a[2] = frag_len.into();
                (a[3], a[4]) = match buf {
                    Some(MemOpBuf::Buf32 { addr, page_cnt }) => (addr.into(), page_cnt.into()),
                    Some(MemOpBuf::Buf64 { addr, page_cnt }) => (addr, page_cnt.into()),
                    None => (0, 0),
                };
            }
            Interface::MemShare {
                total_len,
                frag_len,
                buf,
            } => {
                a[1] = total_len.into();
                a[2] = frag_len.into();
                (a[3], a[4]) = match buf {
                    Some(MemOpBuf::Buf32 { addr, page_cnt }) => (addr.into(), page_cnt.into()),
                    Some(MemOpBuf::Buf64 { addr, page_cnt }) => (addr, page_cnt.into()),
                    None => (0, 0),
                };
            }
            Interface::MemRetrieveReq {
                total_len,
                frag_len,
                buf,
            } => {
                a[1] = total_len.into();
                a[2] = frag_len.into();
                (a[3], a[4]) = match buf {
                    Some(MemOpBuf::Buf32 { addr, page_cnt }) => (addr.into(), page_cnt.into()),
                    Some(MemOpBuf::Buf64 { addr, page_cnt }) => (addr, page_cnt.into()),
                    None => (0, 0),
                };
            }
            Interface::MemRetrieveResp {
                total_len,
                frag_len,
            } => {
                a[1] = total_len.into();
                a[2] = frag_len.into();
            }
            Interface::MemRelinquish => {}
            Interface::MemReclaim { handle, flags } => {
                let handle_regs: [u32; 2] = handle.into();
                a[1] = handle_regs[0].into();
                a[2] = handle_regs[1].into();
                a[3] = flags.into();
            }
            Interface::MemPermGet { addr, page_cnt } => {
                a[1] = match addr {
                    MemAddr::Addr32(addr) => addr.into(),
                    MemAddr::Addr64(addr) => addr,
                };
                a[2] = if version >= Version(1, 3) {
                    page_cnt.unwrap().into()
                } else {
                    assert!(page_cnt.is_none());
                    0
                }
            }
            Interface::MemPermSet {
                addr,
                page_cnt,
                mem_perm,
            } => {
                a[1] = match addr {
                    MemAddr::Addr32(addr) => addr.into(),
                    MemAddr::Addr64(addr) => addr,
                };
                a[2] = page_cnt.into();
                a[3] = mem_perm.into();
            }
            Interface::Success { target_info, args } if version >= Version(1, 2) => {
                a[1] = target_info.into();
                match args {
                    SuccessArgs::Result64_2(regs) => a[2..18].copy_from_slice(&regs[..16]),
                    _ => panic!("{:#x?} requires 8 registers", args),
                }
            }
            Interface::MsgSendDirectReq2 {
                src_id,
                dst_id,
                uuid,
                args,
            } => {
                a[1] = ((src_id as u64) << 16) | dst_id as u64;
                (a[2], a[3]) = uuid.as_u64_pair();
                a[4..18].copy_from_slice(&args.0[..14]);
            }
            Interface::MsgSendDirectResp2 {
                src_id,
                dst_id,
                args,
            } => {
                a[1] = ((src_id as u64) << 16) | dst_id as u64;
                a[2] = 0;
                a[3] = 0;
                a[4..18].copy_from_slice(&args.0[..14]);
            }
            Interface::ConsoleLog {
                char_cnt,
                char_lists,
            } => {
                a[1] = char_cnt.into();
                match char_lists {
                    ConsoleLogChars::Reg32(regs) => {
                        a[2] = regs[0].into();
                        a[3] = regs[1].into();
                        a[4] = regs[2].into();
                        a[5] = regs[3].into();
                        a[6] = regs[4].into();
                        a[7] = regs[5].into();
                    }
                    ConsoleLogChars::Reg64(regs) => a[2..18].copy_from_slice(&regs[..16]),
                }
            }
            _ => unimplemented!("{:#x?} unknown interface", self),
        }
    }

    /// Helper function to create an `FFA_SUCCESS` interface without any arguments.
    pub fn success32_noargs() -> Self {
        Self::Success {
            target_info: 0,
            args: SuccessArgs::Result32([0, 0, 0, 0, 0, 0]),
        }
    }

    /// Helper function to create an `FFA_SUCCESS` interface without any arguments.
    pub fn success64_noargs() -> Self {
        Self::Success {
            target_info: 0,
            args: SuccessArgs::Result64([0, 0, 0, 0, 0, 0]),
        }
    }

    /// Helper function to create an `FFA_ERROR` interface with an error code.
    pub fn error(error_code: FfaError) -> Self {
        Self::Error {
            target_info: TargetInfo {
                endpoint_id: 0,
                vcpu_id: 0,
            },
            error_code,
        }
    }
}

/// Maximum number of characters transmitted in a single `FFA_CONSOLE_LOG32` message.
pub const CONSOLE_LOG_32_MAX_CHAR_CNT: u8 = 24;
/// Maximum number of characters transmitted in a single `FFA_CONSOLE_LOG64` message.
pub const CONSOLE_LOG_64_MAX_CHAR_CNT: u8 = 128;

/// Helper function to convert the "Tightly packed list of characters" format used by the
/// `FFA_CONSOLE_LOG` interface into a byte slice.
pub fn parse_console_log(
    char_cnt: u8,
    char_lists: &ConsoleLogChars,
    log_bytes: &mut [u8],
) -> Result<(), FfaError> {
    match char_lists {
        ConsoleLogChars::Reg32(regs) => {
            if !(1..=CONSOLE_LOG_32_MAX_CHAR_CNT).contains(&char_cnt) {
                return Err(FfaError::InvalidParameters);
            }
            for (i, reg) in regs.iter().enumerate() {
                log_bytes[4 * i..4 * (i + 1)].copy_from_slice(&reg.to_le_bytes());
            }
        }
        ConsoleLogChars::Reg64(regs) => {
            if !(1..=CONSOLE_LOG_64_MAX_CHAR_CNT).contains(&char_cnt) {
                return Err(FfaError::InvalidParameters);
            }
            for (i, reg) in regs.iter().enumerate() {
                log_bytes[8 * i..8 * (i + 1)].copy_from_slice(&reg.to_le_bytes());
            }
        }
    }

    Ok(())
}
