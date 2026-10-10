//! Encrypting secrets at rest with DPAPI: only this Windows user on this computer can read them
//! back, and they are useless in a copied or synced settings file.

use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use windows::core::PCWSTR;

pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
    let input = blob(data);
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: `input` points at `data` for the call; `output` is allocated by the call.
    unsafe { CryptProtectData(&input, PCWSTR::null(), None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut output) }
        .map_err(|err| err.to_string())?;
    Ok(take(output))
}

pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
    let input = blob(data);
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: as above.
    unsafe { CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut output) }
        .map_err(|err| err.to_string())?;
    Ok(take(output))
}

fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr().cast_mut() }
}

/// Copies the bytes DPAPI allocated and frees them.
fn take(output: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    // SAFETY: DPAPI wrote `cbData` bytes at `pbData`, allocated with LocalAlloc.
    let bytes = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe { LocalFree(Some(HLOCAL(output.pbData.cast()))) };
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let sealed = protect(b"ghp_secret").unwrap();
        assert!(!sealed.windows(10).any(|w| w == b"ghp_secret"));
        assert_eq!(unprotect(&sealed).unwrap(), b"ghp_secret");
        assert!(unprotect(b"garbage").is_err());
    }
}
