use std::io;

#[cfg(windows)]
mod imp {
    use super::*;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    use std::slice;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    fn windows_error(error: windows::core::Error) -> io::Error {
        io::Error::other(error)
    }

    fn as_blob(bytes: &mut [u8]) -> io::Result<CRYPT_INTEGER_BLOB> {
        Ok(CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(bytes.len()).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "DPAPI input is too large")
            })?,
            pbData: bytes.as_mut_ptr(),
        })
    }

    unsafe fn take_output(output: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        let bytes = if output.cbData == 0 {
            Vec::new()
        } else {
            // SAFETY: CryptProtectData/CryptUnprotectData returned a valid LocalAlloc buffer.
            unsafe { slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec()
        };
        if !output.pbData.is_null() {
            // SAFETY: DPAPI output buffers must be released with LocalFree.
            unsafe {
                let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
            }
        }
        bytes
    }

    pub fn protect(plaintext: &[u8]) -> io::Result<String> {
        let mut input = plaintext.to_vec();
        let input_blob = as_blob(&mut input)?;
        let mut output = CRYPT_INTEGER_BLOB::default();
        let description: Vec<u16> = "Harness API key\0".encode_utf16().collect();

        // SAFETY: all pointers refer to live buffers for the duration of the call;
        // DPAPI allocates the output buffer, which is released by take_output.
        unsafe {
            CryptProtectData(
                &input_blob,
                PCWSTR(description.as_ptr()),
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
            .map_err(windows_error)?;
            Ok(BASE64.encode(take_output(output)))
        }
    }

    pub fn unprotect(encoded: &str) -> io::Result<Vec<u8>> {
        let encrypted = BASE64
            .decode(encoded)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let mut input = encrypted;
        let input_blob = as_blob(&mut input)?;
        let mut output = CRYPT_INTEGER_BLOB::default();

        // SAFETY: all pointers refer to live buffers for the duration of the call;
        // DPAPI allocates the output buffer, which is released by take_output.
        unsafe {
            CryptUnprotectData(
                &input_blob,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
            .map_err(windows_error)?;
            Ok(take_output(output))
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    fn unsupported() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "DPAPI is only available on Windows",
        )
    }

    pub fn protect(_plaintext: &[u8]) -> io::Result<String> {
        Err(unsupported())
    }

    pub fn unprotect(_encoded: &str) -> io::Result<Vec<u8>> {
        Err(unsupported())
    }
}

pub use imp::{protect, unprotect};

pub fn protect_string(plaintext: &str) -> io::Result<String> {
    protect(plaintext.as_bytes())
}

pub fn unprotect_string(encoded: &str) -> io::Result<String> {
    let bytes = unprotect(encoded)?;
    String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn dpapi_round_trip() {
        let plaintext = "sk-test-中文-\u{1f511}";
        let encrypted = protect_string(plaintext).expect("DPAPI encryption should succeed");
        assert_ne!(encrypted, plaintext);
        assert_eq!(unprotect_string(&encrypted).unwrap(), plaintext);
    }

    #[test]
    fn invalid_ciphertext_is_an_error() {
        assert!(unprotect_string("not valid base64!").is_err());
    }
}
