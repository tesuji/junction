mod utf16;

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io;
use std::mem::{size_of, zeroed, MaybeUninit};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;
use std::ptr::{addr_of_mut, null, null_mut};

pub(crate) use utf16::utf16s;

use super::c;

// See more in <https://learn.microsoft.com/en-us/windows/win32/secauthz/privilege-constants>.
pub static SE_CREATE_SYMBOLIC_LINK_NAME: [u16; 30] = utf16s(b"SeCreateSymbolicLinkPrivilege\0");

pub fn open_reparse_point(reparse_point: &Path, write: bool) -> io::Result<File> {
    let access = c::GENERIC_READ | if write { c::GENERIC_WRITE } else { 0 };
    let mut opts = OpenOptions::new();
    opts.access_mode(access)
        .share_mode(0)
        .custom_flags(c::FILE_FLAG_OPEN_REPARSE_POINT | c::FILE_FLAG_BACKUP_SEMANTICS);
    match opts.open(reparse_point) {
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            // FSCTL_SET_REPARSE_POINT requires SE_CREATE_SYMBOLIC_LINK_NAME privilege
            set_privilege()?;
            opts.open(reparse_point)
        }
        other => other,
    }
}

fn set_privilege() -> io::Result<()> {
    const ERROR_NOT_ALL_ASSIGNED: u32 = 1300;
    const TOKEN_PRIVILEGES_SIZE: u32 = size_of::<c::TOKEN_PRIVILEGES>() as _;
    unsafe {
        let mut handle: c::HANDLE = 0;
        if c::OpenProcessToken(c::GetCurrentProcess(), c::TOKEN_ADJUST_PRIVILEGES, &mut handle) == 0 {
            return Err(io::Error::last_os_error());
        }
        let handle = scopeguard::guard(handle, |h| {
            c::CloseHandle(h);
        });
        let mut tp: c::TOKEN_PRIVILEGES = zeroed();
        let name = SE_CREATE_SYMBOLIC_LINK_NAME.as_ptr();
        if c::LookupPrivilegeValueW(null(), name, &mut tp.Privileges[0].Luid) == 0 {
            return Err(io::Error::last_os_error());
        }
        tp.PrivilegeCount = 1;
        tp.Privileges[0].Attributes = c::SE_PRIVILEGE_ENABLED;
        if c::AdjustTokenPrivileges(*handle, c::FALSE, &tp, TOKEN_PRIVILEGES_SIZE, null_mut(), null_mut()) == 0 {
            return Err(io::Error::last_os_error());
        }
        if c::GetLastError() == ERROR_NOT_ALL_ASSIGNED {
            return Err(io::Error::from_raw_os_error(ERROR_NOT_ALL_ASSIGNED as i32));
        }
    }
    Ok(())
}

pub fn get_reparse_data_point(handle: c::HANDLE, rdb: *mut c::REPARSE_DATA_BUFFER) -> io::Result<()> {
    // Call DeviceIoControl to get the reparse point data
    let mut bytes_returned: u32 = 0;
    if unsafe {
        c::DeviceIoControl(
            handle,
            c::FSCTL_GET_REPARSE_POINT,
            null_mut(),
            0,
            rdb.cast(),
            c::MAXIMUM_REPARSE_DATA_BUFFER_SIZE,
            &mut bytes_returned,
            null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn set_reparse_point(handle: c::HANDLE, rdb: *mut c::REPARSE_DATA_BUFFER, len: u32) -> io::Result<()> {
    let mut bytes_returned: u32 = 0;
    if unsafe {
        c::DeviceIoControl(
            handle,
            c::FSCTL_SET_REPARSE_POINT,
            rdb.cast(),
            len,
            null_mut(),
            0,
            &mut bytes_returned,
            null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

// See https://msdn.microsoft.com/en-us/library/windows/desktop/aa364560(v=vs.85).aspx
pub fn delete_reparse_point(handle: c::HANDLE) -> io::Result<()> {
    let mut rgdb: c::REPARSE_GUID_DATA_BUFFER = unsafe { zeroed() };
    rgdb.ReparseTag = c::IO_REPARSE_TAG_MOUNT_POINT;
    let mut bytes_returned: u32 = 0;

    if unsafe {
        c::DeviceIoControl(
            handle,
            c::FSCTL_DELETE_REPARSE_POINT,
            addr_of_mut!(rgdb).cast(),
            u32::from(c::REPARSE_GUID_DATA_BUFFER_HEADER_SIZE),
            null_mut(),
            0,
            &mut bytes_returned,
            null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn os_str_to_utf16(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

type MaybeU16 = MaybeUninit<u16>;
// Returns the len of buf when success.
// Ref: <rust-lang/rust/src/libstd/sys/windows/mod.rs#L106>.
pub fn get_full_path(target: &Path) -> io::Result<Vec<u16>> {
    let path = os_str_to_utf16(target.as_os_str());
    let file_part = null_mut();
    const U16_UNINIT: MaybeU16 = MaybeU16::uninit();
    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
    // Start off with a stack buf but then spill over to the heap if we end up
    // needing more space.
    //
    // This initial size also works around `GetFullPathNameW` returning
    // incorrect size hints for some short paths:
    // https://github.com/dylni/normpath/issues/5
    let mut stack_buf: [MaybeU16; 512] = [U16_UNINIT; 512];
    let mut heap_buf: Vec<MaybeU16> = Vec::new();
    unsafe {
        let mut n = stack_buf.len();
        loop {
            let buf = if n <= stack_buf.len() {
                &mut stack_buf[..]
            } else {
                let extra = n - heap_buf.len();
                heap_buf.reserve(extra);
                // We used `reserve` and not `reserve_exact`, so in theory we
                // may have gotten more than requested. If so, we'd like to use
                // it... so long as we won't cause overflow.
                n = heap_buf.capacity().min(u32::MAX as usize);
                // Safety: MaybeUninit<u16> does not need initialization
                heap_buf.set_len(n);
                &mut heap_buf[..]
            };

            c::SetLastError(0);
            let k = c::GetFullPathNameW(
                path.as_ptr().cast::<u16>(),
                n as u32,
                maybe_slice_to_ptr(buf),
                file_part,
            ) as usize;
            if k == 0 {
                return Err(crate::io::Error::last_os_error());
            }
            if c::GetLastError() == ERROR_INSUFFICIENT_BUFFER {
                n = n.saturating_mul(2).min(u32::MAX as usize);
            } else if k > n {
                n = k;
            } else {
                // Safety: First `k` values are initialized.
                let slice: &[u16] = maybe_slice_assume_init(&buf[..k]);
                return Ok(slice.into());
            }
        }
    }
}

fn maybe_slice_to_ptr(s: &mut [MaybeU16]) -> *mut u16 {
    // SAFETY: `MaybeUninit<T>` and T are guaranteed to have the same layout
    s.as_mut_ptr() as *mut u16
}

fn maybe_slice_assume_init(s: &[MaybeU16]) -> &[u16] {
    // SAFETY: `MaybeUninit<T>` and T are guaranteed to have the same layout
    unsafe { &*(s as *const [MaybeU16] as *const [u16]) }
}
