//! The history master key lives in Windows Credential Manager, which encrypts it
//! with DPAPI, bound to the signed-in Windows user.

use super::util::{wide, win32_error};
use crate::PlatformError;
use std::ptr::{null, null_mut};
use windows_sys::Win32::{
    Foundation::{ERROR_NOT_FOUND, FILETIME, GetLastError},
    Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
        CredReadW, CredWriteW,
    },
};
use zeroize::Zeroizing;

/// Read the 32-byte key stored under `target`, creating a random one on first run.
pub fn load_or_create_master_key(target: &str) -> Result<Zeroizing<[u8; 32]>, PlatformError> {
    let name = wide(target);
    let mut cred: *mut CREDENTIALW = null_mut();
    // SAFETY: `name` is NUL-terminated; on success `cred` points to a system-owned
    // CREDENTIALW that must be released with CredFree.
    if unsafe { CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut cred) } != 0 {
        // SAFETY: `cred` is valid until CredFree; the blob pointer/size describe a readable buffer.
        let key = unsafe {
            let c = &*cred;
            let key = (c.CredentialBlobSize == 32 && !c.CredentialBlob.is_null()).then(|| {
                let mut k = Zeroizing::new([0u8; 32]);
                k.copy_from_slice(std::slice::from_raw_parts(c.CredentialBlob, 32));
                k
            });
            CredFree(cred.cast());
            key
        };
        return key.ok_or_else(|| {
            PlatformError::Other("the stored history key has an unexpected size".into())
        });
    }
    // SAFETY: no preconditions.
    if unsafe { GetLastError() } != ERROR_NOT_FOUND {
        return Err(win32_error("CredReadW"));
    }

    let mut key = Zeroizing::new([0u8; 32]);
    getrandom::fill(&mut *key)
        .map_err(|e| PlatformError::Other(format!("random key generation failed: {e}")))?;
    let user = wide("zlip");
    let cred = CREDENTIALW {
        Flags: 0,
        Type: CRED_TYPE_GENERIC,
        TargetName: name.as_ptr().cast_mut(),
        Comment: null_mut(),
        LastWritten: FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        },
        CredentialBlobSize: 32,
        CredentialBlob: key.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: null_mut(),
        TargetAlias: null_mut(),
        UserName: user.as_ptr().cast_mut(),
    };
    // SAFETY: all pointers in `cred` reference live buffers for the duration of the call.
    if unsafe { CredWriteW(&cred, 0) } == 0 {
        return Err(win32_error("CredWriteW"));
    }
    Ok(key)
}

/// Remove the stored key (used by "reset all data").
pub fn delete_master_key(target: &str) -> Result<(), PlatformError> {
    let name = wide(target);
    // SAFETY: `name` is NUL-terminated.
    if unsafe { CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
        // SAFETY: no preconditions.
        if unsafe { GetLastError() } == ERROR_NOT_FOUND {
            return Ok(());
        }
        return Err(win32_error("CredDeleteW"));
    }
    let _ = null::<u8>();
    Ok(())
}
