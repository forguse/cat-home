use std::{io, mem::size_of_val, os::windows::ffi::OsStrExt, path::Path, ptr::null_mut};

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

fn command_for(exe: &Path) -> Vec<u16> {
    let quote = u16::from(b'"');
    let mut command = vec![quote];
    command.extend(exe.as_os_str().encode_wide());
    command.extend([quote, 0]);
    command
}

fn registered_command() -> io::Result<Option<Vec<u16>>> {
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
        ERROR_SUCCESS if bytes % 2 == 0 => {
            Ok(registered.get(..bytes as usize / 2).map(<[u16]>::to_vec))
        }
        ERROR_SUCCESS | ERROR_FILE_NOT_FOUND | ERROR_MORE_DATA | ERROR_UNSUPPORTED_TYPE => Ok(None),
        _ => Err(io::Error::from_raw_os_error(status as i32)),
    }
}

pub fn enabled() -> io::Result<bool> {
    let Some(registered) = registered_command()? else {
        return Ok(false);
    };
    Ok(registered == command_for(&std::env::current_exe()?))
}

fn is_renamed_install(exe: &Path, registered: &[u16]) -> bool {
    matches!(
        exe.file_name().and_then(|name| name.to_str()),
        Some("liuxi-village-windows-x64.exe" | "liuxi-village.exe")
    ) && [
        "cat-home-windows-x64.exe",
        "cat-home.exe",
        "猫咪小屋.exe",
        "柳溪村.exe",
    ]
    .iter()
    .any(|name| registered == command_for(&exe.with_file_name(name)))
}

pub fn migrate_executable_name() -> io::Result<()> {
    if let Some(registered) = registered_command()?
        && is_renamed_install(&std::env::current_exe()?, &registered)
    {
        set_enabled(true)?;
    }
    Ok(())
}

pub fn set_enabled(enabled: bool) -> io::Result<()> {
    let status = if enabled {
        let command = command_for(&std::env::current_exe()?);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_follows_the_new_name_only_in_the_existing_install_folder() {
        let new = Path::new(r"C:\桌面 小村\liuxi-village-windows-x64.exe");
        let old = command_for(&new.with_file_name("cat-home-windows-x64.exe"));
        assert!(is_renamed_install(new, &old));
        assert!(is_renamed_install(
            new,
            &command_for(&new.with_file_name("猫咪小屋.exe"))
        ));
        assert!(!is_renamed_install(
            new,
            &command_for(Path::new(r"C:\Downloads\cat-home-windows-x64.exe"))
        ));
        assert!(!is_renamed_install(new, &[]));
        assert!(!is_renamed_install(
            &new.with_file_name("preview.exe"),
            &old
        ));
    }
}
