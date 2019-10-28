use super::types::REPARSE_GUID_DATA_BUFFER_HEADER_SIZE;
use super::types::{ReparseDataBuffer, ReparseGuidDataBuffer};

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;

use scopeguard::ScopeGuard;
use winapi::um::errhandlingapi::{GetLastError, SetLastError};
use winapi::um::fileapi::{CreateFileW, GetFullPathNameW, OPEN_EXISTING};
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
use winapi::um::ioapiset::DeviceIoControl;
use winapi::um::winbase::{FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT};
use winapi::um::winioctl::{FSCTL_DELETE_REPARSE_POINT, FSCTL_GET_REPARSE_POINT, FSCTL_SET_REPARSE_POINT};
use winapi::um::winnt::{FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE};
use winapi::um::winnt::{HANDLE, IO_REPARSE_TAG_MOUNT_POINT, MAXIMUM_REPARSE_DATA_BUFFER_SIZE};

pub fn open_reparse_point(reparse_point: &Path, access_mode: u32) -> io::Result<ScopeGuard<HANDLE, fn(HANDLE)>> {
    let path = os_str_to_utf16(reparse_point.as_os_str());
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            access_mode,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            ptr::null_mut(),
        )
    };
    if ptr::eq(handle, INVALID_HANDLE_VALUE) {
        return Err(io::Error::last_os_error());
    }
    Ok(scopeguard::guard(handle, close_winnt_handle))
}

pub fn get_reparse_data_point<'a>(
    handle: HANDLE,
    data: &'a mut [u8; MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize],
) -> io::Result<&'a ReparseDataBuffer> {
    // Redefine the above char array into a ReparseDataBuffer we can work with
    #[warn(clippy::cast_ptr_alignment)]
    let rdb = data.as_mut_ptr().cast::<ReparseDataBuffer>();
    // Call DeviceIoControl to get the reparse point data
    let mut bytes_returned: u32 = 0;
    if unsafe {
        DeviceIoControl(
            handle,
            FSCTL_GET_REPARSE_POINT,
            ptr::null_mut(),
            0,
            reparse_data.cast(),
            MAXIMUM_REPARSE_DATA_BUFFER_SIZE,
            &mut bytes_returned,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok({ unsafe { &*rdb } })
}

pub fn set_reparse_point(handle: HANDLE, reparse_data: *mut ReparseDataBuffer, len: u32) -> io::Result<()> {
    let mut bytes_returned: u32 = 0;
    if unsafe {
        DeviceIoControl(
            handle,
            FSCTL_SET_REPARSE_POINT,
            reparse_data.cast(),
            len,
            ptr::null_mut(),
            0,
            &mut bytes_returned,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

// See https://msdn.microsoft.com/en-us/library/windows/desktop/aa364560(v=vs.85).aspx
pub fn delete_reparse_point(handle: HANDLE) -> io::Result<()> {
    let mut rgdb: ReparseGuidDataBuffer = unsafe { std::mem::zeroed() };
    rgdb.reparse_tag = IO_REPARSE_TAG_MOUNT_POINT;
    let mut bytes_returned: u32 = 0;

    if unsafe {
        DeviceIoControl(
            handle,
            FSCTL_DELETE_REPARSE_POINT,
            (&mut rgdb as *mut ReparseGuidDataBuffer).cast(),
            u32::from(REPARSE_GUID_DATA_BUFFER_HEADER_SIZE),
            ptr::null_mut(),
            0,
            &mut bytes_returned,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn close_winnt_handle(handle: HANDLE) {
    unsafe {
        CloseHandle(handle);
    }
}

fn os_str_to_utf16(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

// Many Windows APIs follow a pattern of where we hand a buffer and then they
// will report back to us how large the buffer should be or how many bytes
// currently reside in the buffer. This function is an abstraction over these
// functions by making them easier to call.
//
// The first callback, `f1`, is yielded a (pointer, len) pair which can be
// passed to a syscall. The `ptr` is valid for `len` items (u16 in this case).
// The closure is expected to return what the syscall returns which will be
// interpreted by this function to determine if the syscall needs to be invoked
// again (with more buffer space).
//
// Once the syscall has completed (errors bail out early) the second closure is
// yielded the data which has been read from the syscall. The return value
// from this closure is then the return value of the function.
//
// Taken from rust-lang/rust/src/libstd/sys/windows/mod.rs#L106
fn fill_utf16_buf<F1, F2, T>(mut f1: F1, f2: F2) -> io::Result<T>
where
    F1: FnMut(*mut u16, u32) -> u32,
    F2: FnOnce(&[u16]) -> T,
{
    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
    // Start off with a stack buf but then spill over to the heap if we end up
    // needing more space.
    let mut stack_buf = [0u16; 512];
    let mut heap_buf = Vec::new();
    unsafe {
        let mut n = stack_buf.len();
        loop {
            let buf = if n <= stack_buf.len() {
                &mut stack_buf[..]
            } else {
                let extra = n - heap_buf.len();
                heap_buf.reserve(extra);
                heap_buf.set_len(n);
                &mut heap_buf[..]
            };

            // This function is typically called on windows API functions which
            // will return the correct length of the string, but these functions
            // also return the `0` on error. In some cases, however, the
            // returned "correct length" may actually be 0!
            //
            // To handle this case we call `SetLastError` to reset it to 0 and
            // then check it again if we get the "0 error value". If the "last
            // error" is still 0 then we interpret it as a 0 length buffer and
            // not an actual error.
            SetLastError(0);
            let k = match f1(buf.as_mut_ptr(), n as u32) {
                0 if GetLastError() == 0 => 0,
                0 => return Err(io::Error::last_os_error()),
                n => n,
            } as usize;
            if k == n && GetLastError() == ERROR_INSUFFICIENT_BUFFER {
                n *= 2;
            } else if k >= n {
                n = k;
            } else {
                return Ok(f2(&buf[..k]));
            }
        }
    }
}

pub fn get_full_path(target: &Path) -> io::Result<Vec<u16>> {
    let path = os_str_to_utf16(target.as_os_str());
    let file_part: *mut u16 = ptr::null_mut();
    #[warn(clippy::cast_ptr_alignment)]
    fill_utf16_buf(
        |buf, sz| unsafe { GetFullPathNameW(path.as_ptr(), sz, buf, file_part.cast()) },
        |buf| buf.into(),
    )
}
