use std::io;

#[cfg(windows)]
mod imp {
    use super::*;
    use std::mem::size_of;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    struct TokenHandle(HANDLE);

    impl Drop for TokenHandle {
        fn drop(&mut self) {
            // SAFETY: this handle was returned by OpenProcessToken and is owned here.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    pub fn try_is_admin() -> io::Result<bool> {
        let mut token = HANDLE::default();
        // SAFETY: GetCurrentProcess returns a pseudo-handle; token is initialized by
        // OpenProcessToken and closed exactly once by TokenHandle.
        unsafe {
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
                .map_err(io::Error::other)?;
        }
        let token = TokenHandle(token);
        let mut elevation = TOKEN_ELEVATION::default();
        let mut return_length = 0_u32;

        // SAFETY: elevation is a valid writable TOKEN_ELEVATION output buffer.
        unsafe {
            GetTokenInformation(
                token.0,
                TokenElevation,
                Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
                size_of::<TOKEN_ELEVATION>() as u32,
                &mut return_length,
            )
            .map_err(io::Error::other)?;
        }
        Ok(elevation.TokenIsElevated != 0)
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn quote_argument(value: &str) -> String {
        let mut quoted = String::from("\"");
        let mut backslashes = 0;
        for character in value.chars() {
            if character == '\\' {
                backslashes += 1;
                continue;
            }
            if character == '"' {
                quoted.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                quoted.push('"');
            } else {
                quoted.extend(std::iter::repeat_n('\\', backslashes));
                quoted.push(character);
            }
            backslashes = 0;
        }
        quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
        quoted.push('"');
        quoted
    }

    pub fn restart_as_admin(session_id: &str) -> io::Result<()> {
        if session_id.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "session_id cannot be empty",
            ));
        }
        let executable = std::env::current_exe()?;
        let operation = wide("runas");
        let file = wide(&executable.to_string_lossy());
        let parameters = wide(&format!("--resume {}", quote_argument(session_id)));

        // SAFETY: all UTF-16 strings are NUL-terminated and remain alive for the call.
        let result = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(operation.as_ptr()),
                PCWSTR(file.as_ptr()),
                PCWSTR(parameters.as_ptr()),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        let code = result.0 as isize;
        if code <= 32 {
            return Err(io::Error::other(format!(
                "ShellExecuteW runas failed with code {code}"
            )));
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::quote_argument;

        #[test]
        fn quotes_resume_id_as_one_windows_argument() {
            assert_eq!(quote_argument("abc-123"), "\"abc-123\"");
            assert_eq!(quote_argument("a b"), "\"a b\"");
            assert_eq!(quote_argument("a\"b"), "\"a\\\"b\"");
            assert_eq!(quote_argument("a\\"), "\"a\\\\\"");
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn try_is_admin() -> io::Result<bool> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows elevation is only available on Windows",
        ))
    }

    pub fn restart_as_admin(_session_id: &str) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows elevation is only available on Windows",
        ))
    }
}

pub use imp::{restart_as_admin, try_is_admin};

pub fn is_admin() -> bool {
    try_is_admin().unwrap_or(false)
}
