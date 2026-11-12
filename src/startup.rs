use std::{io, mem::size_of_val, os::windows::ffi::OsStrExt, ptr::null_mut};

use windows_sys::{
    Win32::{
        Foundation::{
            ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SUCCESS, ERROR_UNSUPPORTED_TYPE,
        },
        System::Registry::{
            HKEY_CURRENT_USER, REG_SZ, RRF_NOEXPAND, RRF_RT_REG_SZ, RegDeleteKeyValueW,
            RegGetValueW, RegSetKeyValueW,
        },
    },
    core::PCWSTR,
    w,
};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE_NAME: PCWSTR = w!("CodexCatHome");
const MAX_RUN_COMMAND: usize = 260;

fn command() -> io::Result<Vec<u16>> {
    let exe = std::env::current_exe()?;
    let quote = u16::from(b'"');
    let mut command = vec![quote];
    command.extend(exe.as_os_str().encode_wide());
    command.extend([quote, 0]);
    Ok(command)
}

pub fn enabled() -> io::Result<bool> {
    let mut registered = [0u16; MAX_RUN_COMMAND + 1];
    let mut bytes = size_of_val(&registered) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE_NAME,
            RRF_RT_REG_SZ | RRF_NOEXPAND,
            null_mut(),
            registered.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    match status {
        ERROR_SUCCESS => {
            let expected = command()?;
            Ok(bytes as usize == size_of_val(expected.as_slice())
                && registered.starts_with(&expected))
        }
        ERROR_FILE_NOT_FOUND | ERROR_MORE_DATA | ERROR_UNSUPPORTED_TYPE => Ok(false),
        _ => Err(io::Error::from_raw_os_error(status as i32)),
    }
}

pub fn set_enabled(enabled: bool) -> io::Result<()> {
    let status = if enabled {
        let command = command()?;
        if command.len() - 1 > MAX_RUN_COMMAND {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Executable path is too long for Windows login startup",
            ));
        }
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                VALUE_NAME,
                REG_SZ,
                command.as_ptr().cast(),
                size_of_val(command.as_slice()) as u32,
            )
        }
    } else {
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE_NAME) }
    };
    if status == ERROR_SUCCESS || (!enabled && status == ERROR_FILE_NOT_FOUND) {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status as i32))
    }
}
