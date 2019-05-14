//! Library for working with NTFS junctions.
//!
//! Junction Points are a little known NTFS v5+ feature roughly equivalent to UNIX
//! symbolic links. They are supported in Windows 2000 and onwards but cannot be
//! accessed without special tools.
#![cfg(windows)]
#![deny(rust_2018_idioms)]

mod helpers;

use crate::helpers::{
    ReparseDataBuffer, MOUNT_POINT_REPARSE_BUFFER_HEADER_SIZE, REPARSE_DATA_BUFFER_HEADER_SIZE,
    REPARSE_GUID_DATA_BUFFER_HEADER_SIZE,
};
use lazy_static::lazy_static;
use scopeguard::ScopeGuard;
use std::{
    ffi::OsStr,
    io,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr,
};
use winapi::um::{
    errhandlingapi, fileapi, handleapi, ioapiset::DeviceIoControl, winbase, winioctl, winnt,
};

lazy_static! {
    /// This prefix indicates to NTFS that the path is to be treated as a non-interpreted
    /// path in the virtual file system.
    static ref NON_INTERPRETED_PATH_PREFIX: Box<[u16]> = OsStr::new(r"\??\").encode_wide().collect();
}
const NON_INTERPRETED_PATH_PREFIX_SIZE: u16 = 4;
const WCHAR_SIZE: u16 = std::mem::size_of::<u16>() as _;
const WIN32_SYSCALL_FAIL: i32 = 0;

/// Creates a junction point from the specified directory to the specified target directory.
///
/// N.B. Only works on NTFS.
///
/// # Example
///
/// ```rust
/// use std::io;
/// use std::path::Path;
/// # use std::fs;
/// # use junction::create;
/// fn main() -> io::Result<()> {
///     let tmpdir = tempfile::tempdir()?;
///     let target = tmpdir.path().join("target");
///     let junction = tmpdir.path().join("junction");
///     # fs::create_dir_all(&target)?;
///     create(&target, &junction)
/// }
/// ```
pub fn create<P, Q>(target: P, junction: Q) -> io::Result<()>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    use std::fs;

    const UNICODE_NULL_SIZE: u16 = WCHAR_SIZE;
    const MAX_AVAILABLE_PATH_BUFFER: u16 = winnt::MAXIMUM_REPARSE_DATA_BUFFER_SIZE as u16
        - REPARSE_DATA_BUFFER_HEADER_SIZE
        - MOUNT_POINT_REPARSE_BUFFER_HEADER_SIZE
        - 2 * UNICODE_NULL_SIZE;

    fn inner(target: &Path, junction: &Path) -> io::Result<()> {
        // We're using low-level APIs to create the junction, and these are more picky about paths.
        // For example, forward slashes cannot be used as a path separator, so we should try to
        // canonicalize the path first.
        let target = get_full_path(target)?;
        fs::create_dir(junction)?;
        let handle = open_reparse_point(junction, winnt::GENERIC_READ | winnt::GENERIC_WRITE)?;
        // "\??\" + target
        let target_wchar: Vec<u16> = NON_INTERPRETED_PATH_PREFIX
            .iter()
            .chain(target.iter())
            .cloned()
            .collect();
        // Len without `UNICODE_NULL` at the end
        let target_len_in_bytes = target_wchar.len() as u16 * WCHAR_SIZE;
        // Check if `target_wchar.len()` may lead to a buffer overflow.
        if target_len_in_bytes > MAX_AVAILABLE_PATH_BUFFER {
            return Err(io::Error::new(io::ErrorKind::Other, "`target` is too long"));
        }
        let in_buffer_size: u16;
        // Redefine the above char array into a ReparseDataBuffer we can work with
        let mut data = [0u8; winnt::MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize];
        #[allow(clippy::cast_ptr_alignment)]
        let rdb = data.as_mut_ptr() as *mut ReparseDataBuffer;
        unsafe {
            let rdb = &mut *rdb;
            // Set the type of reparse point we are creating
            rdb.reparse_tag = winnt::IO_REPARSE_TAG_MOUNT_POINT;
            rdb.reserved = 0;

            // Copy the junction's target
            rdb.reparse_buffer.substitute_name_offset = 0;
            rdb.reparse_buffer.substitute_name_length = target_len_in_bytes;

            // Copy the junction's link name
            rdb.reparse_buffer.print_name_offset = target_len_in_bytes + UNICODE_NULL_SIZE;
            rdb.reparse_buffer.print_name_length = 0;

            // Safe because we checked `MAX_AVAILABLE_PATH_BUFFER`
            ptr::copy_nonoverlapping(
                target_wchar.as_ptr() as *const u16,
                rdb.reparse_buffer.path_buffer.as_mut_ptr() as _,
                target_wchar.len(),
            );

            // Set the total size of the data buffer
            rdb.reparse_data_length = target_len_in_bytes
                + MOUNT_POINT_REPARSE_BUFFER_HEADER_SIZE
                + 2 * UNICODE_NULL_SIZE;
            in_buffer_size = rdb.reparse_data_length + REPARSE_DATA_BUFFER_HEADER_SIZE;
        }

        set_reparse_point(*handle, rdb, u32::from(in_buffer_size))
    }
    inner(target.as_ref(), junction.as_ref())
}

/// Deletes a `junction` reparse point from the specified file or directory.
///
/// N.B. Only works on NTFS.
///
/// This function does not delete the file or directory. Also it does nothing
/// if the `junction` point does not exist.
///
/// # Example
///
/// ```rust
/// use std::io;
/// use std::path::Path;
/// # use std::fs;
/// # use junction::{create, delete};
/// fn main() -> io::Result<()> {
///     let tmpdir = tempfile::tempdir()?;
///     let target = tmpdir.path().join("target");
///     let junction = tmpdir.path().join("junction");
///     # fs::create_dir_all(&target)?;
///     create(&target, &junction)?;
///     delete(&junction)
/// }
/// ```
pub fn delete<P: AsRef<Path>>(junction: P) -> io::Result<()> {
    fn inner(junction: &Path) -> io::Result<()> {
        let handle = open_reparse_point(junction, winnt::GENERIC_READ | winnt::GENERIC_WRITE)?;
        delete_reparse_point(*handle)
    }
    inner(junction.as_ref())
}

/// Determines whether the specified path exists and refers to a junction point.
///
/// # Example
///
/// ```rust
/// use std::io;
/// # use junction::exists;
/// fn main() -> io::Result<()> {
///     assert!(exists(r"C:\Users\Default User")?);
///     Ok(())
/// }
/// ```
pub fn exists<P: AsRef<Path>>(junction: P) -> io::Result<bool> {
    fn inner(junction: &Path) -> io::Result<bool> {
        if !junction.exists() {
            return Ok(false);
        }
        let handle = open_reparse_point(junction, winnt::GENERIC_READ)?;
        // Allocate enough space to fit the maximum sized reparse data buffer
        let mut data = [0u8; winnt::MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize];
        // RedefKine the above char array into a ReparseDataBuffer we can work with
        let rdb = get_reparse_data_point(*handle, &mut data)?;
        // The reparse tag indicates if this is a junction or not
        Ok(rdb.reparse_tag == winnt::IO_REPARSE_TAG_MOUNT_POINT)
    }
    inner(junction.as_ref())
}

/// Gets the target of the specified junction point.
///
/// N.B. Only works on NTFS.
///
/// # Example
///
/// ```rust
/// use std::io;
/// # use junction::get_target;
/// fn main() -> io::Result<()> {
///     assert_eq!(get_target(r"C:\Users\Default User")?.to_str(), Some(r"C:\Users\Default"));
///     Ok(())
/// }
/// ```
pub fn get_target<P: AsRef<Path>>(junction: P) -> io::Result<PathBuf> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, slice};

    fn inner(junction: &Path) -> io::Result<PathBuf> {
        if !junction.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "`junction` does not exist",
            ));
        }
        let handle = open_reparse_point(junction, winnt::GENERIC_READ)?;
        let mut data = [0u8; winnt::MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize];
        // RedefKine the above char array into a ReparseDataBuffer we can work with
        let rdb = get_reparse_data_point(*handle, &mut data)?;
        if rdb.reparse_tag == winnt::IO_REPARSE_TAG_MOUNT_POINT {
            let offset = rdb.reparse_buffer.substitute_name_offset / WCHAR_SIZE;
            let len = rdb.reparse_buffer.substitute_name_length / WCHAR_SIZE;
            let mut wide = unsafe {
                let buf = rdb.reparse_buffer.path_buffer.as_ptr().add(offset as usize);
                slice::from_raw_parts(buf, len as usize)
            };
            // In case of "\??\C:\foo\bar"
            if wide.starts_with(&NON_INTERPRETED_PATH_PREFIX) {
                wide = &wide[(NON_INTERPRETED_PATH_PREFIX_SIZE as usize)..];
            }
            Ok(PathBuf::from(OsString::from_wide(wide)))
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "not a reparse tag mount point",
            ))
        }
    }
    inner(junction.as_ref())
}

fn open_reparse_point(
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

fn get_reparse_data_point<'a>(
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

fn set_reparse_point(
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
fn delete_reparse_point(handle: winnt::HANDLE) -> io::Result<()> {
    use crate::helpers::ReparseGuidDataBuffer;
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
// Taken from src/libstd/sys/windows/mod.rs#L106
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

fn get_full_path(target: &Path) -> io::Result<Vec<u16>> {
    let path = os_str_to_utf16(target.as_os_str());
    let file_part: *mut u16 = ptr::null_mut();
    #[allow(clippy::cast_ptr_alignment)]
    fill_utf16_buf(
        |buf, sz| unsafe { fileapi::GetFullPathNameW(path.as_ptr(), sz, buf, file_part as _) },
        |buf| buf.into(),
    )
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, File},
        io::{self, Write},
        os::windows::fs::symlink_file,
    };

    // https://docs.microsoft.com/en-us/windows/desktop/debug/system-error-codes
    const ERROR_NOT_A_REPARSE_POINT: i32 = 0x1126;
    const ERROR_ALREADY_EXISTS: i32 = 0xb7;

    macro_rules! check {
        ($e:expr) => {
            match $e {
                Ok(t) => t,
                Err(e) => panic!("{} failed with: {}", stringify!($e), e),
            }
        };
    }

    fn create_tempdir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("junction-test-")
            .tempdir_in("target/debug")
            .unwrap()
    }

    #[test]
    fn create_dir_all_with_junctions() {
        let tmpdir = create_tempdir();
        let target = tmpdir.path().join("target");

        let junction = tmpdir.path().join("junction");
        let b = junction.join("a/b");

        fs::create_dir_all(&target).unwrap();

        check!(super::create(&target, &junction));
        check!(fs::create_dir_all(&b));
        // the junction itself is not a directory, but `is_dir()` on a Path
        // follows links
        assert!(junction.is_dir());
        assert!(b.exists());
    }

    #[test]
    fn create_recursive_rmdir() {
        let tmpdir = create_tempdir();
        let d1 = tmpdir.path().join("d1"); // "d1"
        let dt = d1.join("t"); // "d1/t"
        let dtt = dt.join("t"); // "d1/t/t"
        let d2 = tmpdir.path().join("d2"); // "d2"
        let canary = d2.join("do_not_delete"); // "d2/do_not_delete"

        check!(fs::create_dir_all(&dtt));
        check!(fs::create_dir_all(&d2));
        check!(check!(File::create(&canary)).write_all(b"foo"));

        check!(super::create(&d2, &dt.join("d2"))); // "d1/t/d2" -> "d2"

        let _ = symlink_file(&canary, &d1.join("canary")); // d1/canary -> d2/do_not_delete
        check!(fs::remove_dir_all(&d1));

        assert!(!d1.is_dir());
        assert!(canary.exists());
    }

    #[test]
    fn create_recursive_rmdir_of_symlink() {
        // test we do not recursively delete a symlink but only dirs.
        let tmpdir = create_tempdir();
        let link = tmpdir.path().join("link");
        let dir = tmpdir.path().join("dir");
        let canary = dir.join("do_not_delete");
        check!(fs::create_dir_all(&dir));
        check!(check!(File::create(&canary)).write_all(b"foo"));
        check!(super::create(&dir, &link));
        check!(fs::remove_dir_all(&link));

        assert!(!link.is_dir());
        assert!(canary.exists());
    }

    #[test]
    fn create_directory_exist_before() {
        let tmpdir = create_tempdir();

        let target = tmpdir.path().join("target");
        let junction = tmpdir.path().join("junction");

        check!(fs::create_dir_all(&junction));

        match super::create(&target, &junction) {
            Err(ref e) if e.raw_os_error() == Some(ERROR_ALREADY_EXISTS) => (),
            _ => panic!("directory exists before creating"),
        }
    }

    #[test]
    fn create_target_no_exist() {
        let tmpdir = create_tempdir();

        let target = tmpdir.path().join("target");
        let junction = tmpdir.path().join("junction");

        match super::create(&target, &junction) {
            Ok(()) => (),
            _ => panic!("junction should point to non exist target path"),
        }
    }

    #[test]
    fn delete_junctions() {
        let tmpdir = create_tempdir();

        let non_existence_dir = tmpdir.path().join("non_existence_dir");
        match super::delete(&non_existence_dir) {
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => (),
            _ => panic!("target path does not exist or is not a directory"),
        }

        let dir_not_junction = tmpdir.path().join("dir_not_junction");
        check!(fs::create_dir_all(&dir_not_junction));
        match super::delete(&dir_not_junction) {
            Err(ref e) if e.raw_os_error() == Some(ERROR_NOT_A_REPARSE_POINT) => (),
            _ => panic!("target path is not a junction point"),
        }

        let file = tmpdir.path().join("foo-file");
        check!(check!(File::create(&file)).write_all(b"foo"));
        match super::delete(&file) {
            Err(ref e) if e.raw_os_error() == Some(ERROR_NOT_A_REPARSE_POINT) => (),
            _ => panic!("target path is not a junction point"),
        }
    }

    #[test]
    fn exists_verify() {
        let tmpdir = create_tempdir();

        // Check no such directory or file
        let no_such_dir = tmpdir.path().join("no_such_dir");
        assert_eq!(check!(super::exists(&no_such_dir)), false);

        // Target exists but not a junction
        let no_such_file = tmpdir.path().join("file");
        check!(check!(File::create(&no_such_file)).write_all(b"foo"));
        match super::exists(&no_such_file) {
            Err(ref e) if e.raw_os_error() == Some(ERROR_NOT_A_REPARSE_POINT) => (),
            _ => panic!("target exists but not a junction"),
        }

        let target = tmpdir.path().join("target");
        let junction = tmpdir.path().join("junction");
        let file = target.join("file");
        let junction_file = junction.join("file");

        check!(fs::create_dir_all(&target));
        check!(check!(File::create(&file)).write_all(b"foo"));

        assert!(
            !junction_file.exists(),
            "file should not be located until junction created"
        );
        assert_eq!(
            check!(super::exists(&junction)),
            false,
            "junction not created yet"
        );

        check!(super::create(&target, &junction));
        assert_eq!(
            check!(super::exists(&junction)),
            true,
            "junction should exist now"
        );
        assert_eq!(&check!(super::get_target(&junction)), &target);
        assert!(
            junction_file.exists(),
            "file should be accessible via the junction"
        );

        check!(super::delete(&junction));
        match super::exists(&junction) {
            Err(ref e) if e.raw_os_error() == Some(ERROR_NOT_A_REPARSE_POINT) => (),
            _ => panic!("junction had been deleted"),
        }
        assert!(
            !junction_file.exists(),
            "file should not be located after junction deleted"
        );
        assert!(junction.exists(), "directory should not be deleted");
    }

    #[test]
    fn get_target_user_dirs() {
        // junction
        assert_eq!(
            check!(super::get_target(r"C:\Users\Default User")).to_str(),
            Some(r"C:\Users\Default"),
        );
        // junction with special permissions
        assert_eq!(
            check!(super::get_target(r"C:\Documents and Settings\")).to_str(),
            Some(r"C:\Users"),
        );

        let tmpdir = create_tempdir();

        let non_existence_dir = tmpdir.path().join("non_existence_dir");
        match super::get_target(&non_existence_dir) {
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => (),
            _ => panic!("target path does not exist or is not a directory"),
        }

        let dir_not_junction = tmpdir.path().join("dir_not_junction");
        check!(fs::create_dir_all(&dir_not_junction));
        match super::get_target(&dir_not_junction) {
            Err(ref e) if e.raw_os_error() == Some(ERROR_NOT_A_REPARSE_POINT) => (),
            _ => panic!("target path is not a junction point"),
        }

        let file = tmpdir.path().join("foo-file");
        check!(check!(File::create(&file)).write_all(b"foo"));
        match super::get_target(&file) {
            Err(ref e) if e.raw_os_error() == Some(ERROR_NOT_A_REPARSE_POINT) => (),
            _ => panic!("target path is not a junction point"),
        }
    }
}
