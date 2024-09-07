mod c;
mod cast;
mod helpers;

use std::ffi::OsString;
use std::mem::size_of;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::ptr::{addr_of_mut, copy_nonoverlapping};
use std::{cmp, fs, io, slice};

use cast::BytesAsReparseDataBuffer;

/// This prefix indicates to NTFS that the path is to be treated as a non-interpreted
/// path in the virtual file system.
const NON_INTERPRETED_PATH_PREFIX: [u16; 4] = helpers::utf16s(br"\??\");

const WCHAR_SIZE: u16 = size_of::<u16>() as _;

pub fn create(target: &Path, junction: &Path) -> io::Result<()> {
    const UNICODE_NULL_SIZE: u16 = WCHAR_SIZE;
    const MAX_AVAILABLE_PATH_BUFFER: u16 = c::MAXIMUM_REPARSE_DATA_BUFFER_SIZE as u16
        - c::REPARSE_DATA_BUFFER_HEADER_SIZE
        - c::MOUNT_POINT_REPARSE_BUFFER_HEADER_SIZE
        - 2 * UNICODE_NULL_SIZE;

    // We're using low-level APIs to create the junction, and these are more picky about paths.
    // For example, forward slashes cannot be used as a path separator, so we should try to
    // canonicalize the path first.
    let target = helpers::get_full_path(target)?;
    fs::create_dir(junction)?;
    let file = helpers::open_reparse_point(junction, true)?;
    let target_len_in_bytes = {
        // "\??\" + target
        let len = NON_INTERPRETED_PATH_PREFIX.len().saturating_add(target.len());
        let min_len = cmp::min(len, u16::MAX as usize) as u16;
        // Len without `UNICODE_NULL` at the end
        let target_len_in_bytes = min_len.saturating_mul(WCHAR_SIZE);
        // Check for buffer overflow.
        if target_len_in_bytes > MAX_AVAILABLE_PATH_BUFFER {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "`target` is too long"));
        }
        target_len_in_bytes
    };

    // Redefine the above char array into a ReparseDataBuffer we can work with
    let mut data = BytesAsReparseDataBuffer::new();
    let rdb = data.as_mut_ptr();
    let in_buffer_size: u16 = unsafe {
        // Set the type of reparse point we are creating
        addr_of_mut!((*rdb).ReparseTag).write(c::IO_REPARSE_TAG_MOUNT_POINT);
        addr_of_mut!((*rdb).Reserved).write(0);

        // We write target at offset 0 of PathBuffer
        addr_of_mut!((*rdb).ReparseBuffer.SubstituteNameOffset).write(0);
        addr_of_mut!((*rdb).ReparseBuffer.SubstituteNameLength).write(target_len_in_bytes);

        // We do not use PrintName. However let's set its offset correctly right after SubstituteName
        addr_of_mut!((*rdb).ReparseBuffer.PrintNameOffset).write(target_len_in_bytes + UNICODE_NULL_SIZE);
        addr_of_mut!((*rdb).ReparseBuffer.PrintNameLength).write(0);

        let mut path_buffer_ptr: *mut u16 = addr_of_mut!((*rdb).ReparseBuffer.PathBuffer).cast();
        // Safe because we checked `MAX_AVAILABLE_PATH_BUFFER`
        copy_nonoverlapping(
            NON_INTERPRETED_PATH_PREFIX.as_ptr(),
            path_buffer_ptr,
            NON_INTERPRETED_PATH_PREFIX.len(),
        );
        // TODO: Do we need to write the NULL-terminator byte?
        // It looks like libuv does that.
        path_buffer_ptr = path_buffer_ptr.add(NON_INTERPRETED_PATH_PREFIX.len());
        copy_nonoverlapping(target.as_ptr(), path_buffer_ptr, target.len());

        // Set the total size of the data buffer
        let size = target_len_in_bytes.wrapping_add(c::MOUNT_POINT_REPARSE_BUFFER_HEADER_SIZE + 2 * UNICODE_NULL_SIZE);
        addr_of_mut!((*rdb).ReparseDataLength).write(size);
        size.wrapping_add(c::REPARSE_DATA_BUFFER_HEADER_SIZE)
    };

    helpers::set_reparse_point(file.as_raw_handle() as isize, rdb, u32::from(in_buffer_size))
}

pub fn delete(junction: &Path) -> io::Result<()> {
    let file = helpers::open_reparse_point(junction, true)?;
    helpers::delete_reparse_point(file.as_raw_handle() as isize)
}

pub fn exists(junction: &Path) -> io::Result<bool> {
    if !junction.exists() {
        return Ok(false);
    }
    let file = helpers::open_reparse_point(junction, false)?;
    // Allocate enough space to fit the maximum sized reparse data buffer
    let mut data = BytesAsReparseDataBuffer::new();
    // XXX: Could also use FindFirstFile to read the reparse point type
    // Ref https://learn.microsoft.com/en-us/windows/win32/fileio/reparse-point-tags
    helpers::get_reparse_data_point(file.as_raw_handle() as isize, data.as_mut_ptr())?;
    // SATETY: rdb should be initialized now
    let rdb = unsafe { data.assume_init() };
    // The reparse tag indicates if this is a junction or not
    Ok(rdb.ReparseTag == c::IO_REPARSE_TAG_MOUNT_POINT)
}

pub fn get_target(junction: &Path) -> io::Result<PathBuf> {
    // MSRV(1.63): use Path::try_exists instead
    if !junction.exists() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "`junction` does not exist"));
    }
    let file = helpers::open_reparse_point(junction, false)?;
    let mut data = BytesAsReparseDataBuffer::new();
    helpers::get_reparse_data_point(file.as_raw_handle() as isize, data.as_mut_ptr())?;
    // SAFETY: rdb should be initialized now
    let rdb = unsafe { data.assume_init() };
    if rdb.ReparseTag == c::IO_REPARSE_TAG_MOUNT_POINT {
        let offset = rdb.ReparseBuffer.SubstituteNameOffset / WCHAR_SIZE;
        let len = rdb.ReparseBuffer.SubstituteNameLength / WCHAR_SIZE;
        let wide = unsafe {
            let buf = rdb.ReparseBuffer.PathBuffer.as_ptr().add(offset as usize);
            slice::from_raw_parts(buf, len as usize)
        };
        // In case of "\??\C:\foo\bar"
        let wide = wide.strip_prefix(&NON_INTERPRETED_PATH_PREFIX).unwrap_or(wide);
        Ok(PathBuf::from(OsString::from_wide(wide)))
    } else {
        Err(io::Error::new(io::ErrorKind::Other, "not a reparse tag mount point"))
    }
}
