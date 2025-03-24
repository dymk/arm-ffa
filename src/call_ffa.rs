// SPDX-FileCopyrightText: Copyright 2025 Arm Limited and/or its affiliates <open-source-office@arm.com>
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{Error, Interface, Version};

pub trait CallFfa: Send + Sync {
    fn call_ffa(&self, version: Version, interface: &Interface) -> Result<(), Error>;
}

pub fn call_ffa(version: Version, interface: &Interface) -> Result<Interface, Error> {
    #[cfg(target_arch = "aarch64")]
    {
        let mut in_regs = [0u64; 18];
        let mut out_regs = [0u64; 18];

        interface.to_regs(version, &mut in_regs);
        // Safety: yolo
        unsafe {
            core::arch::asm!(
                "smc #0",
                inout("x0") in_regs[0] => out_regs[0],
                inout("x1") in_regs[1] => out_regs[1],
                inout("x2") in_regs[2] => out_regs[2],
                inout("x3") in_regs[3] => out_regs[3],
                inout("x4") in_regs[4] => out_regs[4],
                inout("x5") in_regs[5] => out_regs[5],
                inout("x6") in_regs[6] => out_regs[6],
                inout("x7") in_regs[7] => out_regs[7],
                inout("x8") in_regs[8] => out_regs[8],
                inout("x9") in_regs[9] => out_regs[9],
                inout("x10") in_regs[10] => out_regs[10],
                inout("x11") in_regs[11] => out_regs[11],
                inout("x12") in_regs[12] => out_regs[12],
                inout("x13") in_regs[13] => out_regs[13],
                inout("x14") in_regs[14] => out_regs[14],
                inout("x15") in_regs[15] => out_regs[15],
                inout("x16") in_regs[16] => out_regs[16],
                inout("x17") in_regs[17] => out_regs[17],
                options(nomem, nostack)
            );
        };
        Interface::from_regs(version, &out_regs)
    }

    #[cfg(not(target_arch = "aarch64"))]
    {
        let _ = version;
        let _ = interface;
        unimplemented!("Unsupported architecture");
    }
}
