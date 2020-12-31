// SPDX-License-Identifier: Apache-2.0

//! The attestation syscall for Enarx

//#![feature(asm)]
#![deny(missing_docs)]
#![deny(clippy::all)]
#![deny(clippy::cargo)]

use std::io::{Error, ErrorKind, Result};

/// The attestation that was performed
pub enum Attestation {
    /// No attestation was performed
    None,

    /// SEV attestation was performed
    ///
    /// The `usize` field indicates the length of valid output bytes.
    Sev(usize),

    /// SGX attestation was performed
    ///
    /// The `usize` field indicates the length of valid output bytes.
    Sgx(usize),
}

/// Performs an attestation pseudo-syscall
///
/// The `input` parameter contains some bytes to include in an attestation
/// report. This will often be a public key or a cryptographic hash of
/// additional data to bind to the attestation.
///
/// The `output` parameter will contain the output data from the attestation.
/// If the `output` parameter has a zero length, no output data will be written
/// and the return value will hint at the required length for the output buffer.
pub fn attest(input: &[u8], output: &mut [u8]) -> Result<Attestation> {
    let rax: isize;
    let rdx: usize;

    let (ptr, len) = match output.len() {
        0 => (std::ptr::null_mut(), 0),
        x => (output.as_mut_ptr(), x),
    };

    unsafe {
        asm!(
            "syscall",
            in("rax") 0xEA01,
            in("rdi") input.as_ptr(),
            in("rsi") input.len(),
            in("rdx") ptr,
            in("r10") len,
            out("rcx") _,
            out("r11") _,
            lateout("rax") rax,
            lateout("rdx") rdx,
        );
    }

    if rax < 0 {
        return Err(Error::from_raw_os_error(-rax as i32));
    }

    Ok(match rdx {
        0 => Attestation::None,
        1 => Attestation::Sev(rax as _),
        2 => Attestation::Sgx(rax as _),
        _ => return Err(ErrorKind::Other.into()),
    })
}
