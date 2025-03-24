// SPDX-FileCopyrightText: Copyright 2025 Arm Limited and/or its affiliates <open-source-office@arm.com>
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::Error;
use num_enum::{IntoPrimitive, TryFromPrimitive};

/// Function IDs of the various FF-A interfaces.
#[derive(Clone, Copy, Debug, Eq, IntoPrimitive, PartialEq, TryFromPrimitive)]
#[num_enum(error_type(name = Error, constructor = Error::UnrecognisedFunctionId))]
#[repr(u32)]
pub enum FuncId {
    Error = 0x84000060,
    Success32 = 0x84000061,
    Success64 = 0xc4000061,
    Interrupt = 0x84000062,
    Version = 0x84000063,
    Features = 0x84000064,
    RxAcquire = 0x84000084,
    RxRelease = 0x84000065,
    RxTxMap32 = 0x84000066,
    RxTxMap64 = 0xc4000066,
    RxTxUnmap = 0x84000067,
    PartitionInfoGet = 0x84000068,
    PartitionInfoGetRegs = 0xc400008b,
    IdGet = 0x84000069,
    SpmIdGet = 0x84000085,
    ConsoleLog32 = 0x8400008a,
    ConsoleLog64 = 0xc400008a,
    MsgWait = 0x8400006b,
    Yield = 0x8400006c,
    Run = 0x8400006d,
    NormalWorldResume = 0x8400007c,
    MsgSend2 = 0x84000086,
    MsgSendDirectReq32 = 0x8400006f,
    MsgSendDirectReq64 = 0xc400006f,
    MsgSendDirectReq64_2 = 0xc400008d,
    MsgSendDirectResp32 = 0x84000070,
    MsgSendDirectResp64 = 0xc4000070,
    MsgSendDirectResp64_2 = 0xc400008e,
    NotificationBitmapCreate = 0x8400007d,
    NotificationBitmapDestroy = 0x8400007e,
    NotificationBind = 0x8400007f,
    NotificationUnbind = 0x84000080,
    NotificationSet = 0x84000081,
    NotificationGet = 0x84000082,
    NotificationInfoGet32 = 0x84000083,
    NotificationInfoGet64 = 0xc4000083,
    El3IntrHandle = 0x8400008c,
    SecondaryEpRegister32 = 0x84000087,
    SecondaryEpRegister64 = 0xc4000087,
    MemDonate32 = 0x84000071,
    MemDonate64 = 0xc4000071,
    MemLend32 = 0x84000072,
    MemLend64 = 0xc4000072,
    MemShare32 = 0x84000073,
    MemShare64 = 0xc4000073,
    MemRetrieveReq32 = 0x84000074,
    MemRetrieveReq64 = 0xc4000074,
    MemRetrieveResp = 0x84000075,
    MemRelinquish = 0x84000076,
    MemReclaim = 0x84000077,
    MemPermGet32 = 0x84000088,
    MemPermGet64 = 0xc4000088,
    MemPermSet32 = 0x84000089,
    MemPermSet64 = 0xc4000089,
}

impl FuncId {
    /// Returns true if this is a 32-bit call, or false if it is a 64-bit call.
    pub fn is_32bit(&self) -> bool {
        u32::from(*self) & (1 << 30) != 0
    }
}
