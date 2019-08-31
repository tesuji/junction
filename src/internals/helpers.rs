use super::types::ReparseDataBuffer;
use scopeguard::ScopeGuard;
use std::{ffi::OsStr, io, path::Path, ptr};
use winapi::um::{
    errhandlingapi, fileapi, handleapi, ioapiset::DeviceIoControl, winbase, winioctl, winnt,
};

const WIN32_SYSCALL_FAIL: i32 = 0;

pub fn open_reparse_point(
    reparse_point: &Path,
    access_mode: u32,
) -> io::Result<ScopeGuard<winnt::HANDLE, fn(winnt::HANDLE)>> {
    use fileapi::{CreateFileW, OPEN_EXISTING};
    use handleapi::INVALID_HANDLE_VALUE;
    use winbase::{FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT};
    use winnt::{FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE};

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
        Err(io::Error::last_os_error())
    } else {
        Ok(scopeguard::guard(handle, close_winnt_handle))
    }
}

pub fn get_reparse_data_point<'a>(
    handle: winnt::HANDLE,
    data: &'a mut [u8; winnt::MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize],
) -> io::Result<&'a ReparseDataBuffer> {
    // Redefine the above char array into a ReparseDataBuffer we can work with
    #[allow(clippy::cast_ptr_alignment)]
    let reparse_data = data.as_mut_ptr() as *mut ReparseDataBuffer;

    // Call DeviceIoControl to get the reparse point data

    let mut bytes_returned: u32 = 0;
    let result = unsafe {
        DeviceIoControl(
            handle,
            winioctl::FSCTL_GET_REPARSE_POINT,
            ptr::null_mut(),
            0,
            reparse_data as _,
            winnt::MAXIMUM_REPARSE_DATA_BUFFER_SIZE,
            &mut bytes_returned,
            ptr::null_mut(),
        )
    };

    if result == WIN32_SYSCALL_FAIL {
        Err(io::Error::last_os_error())
    } else {
        Ok({ unsafe { &*reparse_data } })
    }
}

pub fn set_reparse_point(
    handle: winnt::HANDLE,
    reparse_data: *mut ReparseDataBuffer,
    len: u32,
) -> io::Result<()> {
    let mut bytes_returned: u32 = 0;
    let result = unsafe {
        DeviceIoControl(
            handle,
            winioctl::FSCTL_SET_REPARSE_POINT,
            reparse_data as _,
            len,
            ptr::null_mut(),
            0,
            &mut bytes_returned,
            ptr::null_mut(),
        )
    };

    if result == WIN32_SYSCALL_FAIL {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

// See https://msdn.microsoft.com/en-us/library/windows/desktop/aa364560(v=vs.85).aspx
pub fn delete_reparse_point(handle: winnt::HANDLE) -> io::Result<()> {
    use super::types::ReparseGuidDataBuffer;
    use super::types::REPARSE_GUID_DATA_BUFFER_HEADER_SIZE;
    let mut rgdb: ReparseGuidDataBuffer = unsafe { std::mem::zeroed() };
    rgdb.reparse_tag = winnt::IO_REPARSE_TAG_MOUNT_POINT;
    let mut bytes_returned: u32 = 0;
    let result = unsafe {
        DeviceIoControl(
            handle,
            winioctl::FSCTL_DELETE_REPARSE_POINT,
            &mut rgdb as *mut ReparseGuidDataBuffer as _,
            u32::from(REPARSE_GUID_DATA_BUFFER_HEADER_SIZE),
            ptr::null_mut(),
            0,
            &mut bytes_returned,
            ptr::null_mut(),
        )
    };

    if result == WIN32_SYSCALL_FAIL {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn close_winnt_handle(handle: winnt::HANDLE) {
    use handleapi::CloseHandle;
    unsafe {
        CloseHandle(handle);
    }
}

fn os_str_to_utf16(s: &OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    let mut maybe_result: Vec<u16> = s.encode_wide().collect();
    maybe_result.push(0);
    maybe_result
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
            errhandlingapi::SetLastError(0);
            let k = match f1(buf.as_mut_ptr(), n as u32) {
                0 if errhandlingapi::GetLastError() == 0 => 0,
                0 => return Err(io::Error::last_os_error()),
                n => n,
            } as usize;
            if k == n && errhandlingapi::GetLastError() == ERROR_INSUFFICIENT_BUFFER {
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
    #[allow(clippy::cast_ptr_alignment)]
    fill_utf16_buf(
        |buf, sz| unsafe { fileapi::GetFullPathNameW(path.as_ptr(), sz, buf, file_part as _) },
        |buf| buf.into(),
    )
}
