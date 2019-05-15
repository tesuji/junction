mod helpers;
mod types;

use self::types::{
    ReparseDataBuffer, MOUNT_POINT_REPARSE_BUFFER_HEADER_SIZE, REPARSE_DATA_BUFFER_HEADER_SIZE,
};

use lazy_static::lazy_static;
use std::{
    ffi::OsStr,
    io,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr,
};
use winapi::um::winnt;

lazy_static! {
    /// This prefix indicates to NTFS that the path is to be treated as a non-interpreted
    /// path in the virtual file system.
    static ref NON_INTERPRETED_PATH_PREFIX: Box<[u16]> = OsStr::new(r"\??\").encode_wide().collect();
}
const NON_INTERPRETED_PATH_PREFIX_SIZE: u16 = 4;
const WCHAR_SIZE: u16 = std::mem::size_of::<u16>() as _;

pub fn create(target: &Path, junction: &Path) -> io::Result<()> {
    use std::fs;

    const UNICODE_NULL_SIZE: u16 = WCHAR_SIZE;
    const MAX_AVAILABLE_PATH_BUFFER: u16 = winnt::MAXIMUM_REPARSE_DATA_BUFFER_SIZE as u16
        - REPARSE_DATA_BUFFER_HEADER_SIZE
        - MOUNT_POINT_REPARSE_BUFFER_HEADER_SIZE
        - 2 * UNICODE_NULL_SIZE;

    // We're using low-level APIs to create the junction, and these are more picky about paths.
    // For example, forward slashes cannot be used as a path separator, so we should try to
    // canonicalize the path first.
    let target = self::helpers::get_full_path(target)?;
    fs::create_dir(junction)?;
    let handle =
        self::helpers::open_reparse_point(junction, winnt::GENERIC_READ | winnt::GENERIC_WRITE)?;
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
        rdb.reparse_data_length =
            target_len_in_bytes + MOUNT_POINT_REPARSE_BUFFER_HEADER_SIZE + 2 * UNICODE_NULL_SIZE;
        in_buffer_size = rdb.reparse_data_length + REPARSE_DATA_BUFFER_HEADER_SIZE;
    }

    self::helpers::set_reparse_point(*handle, rdb, u32::from(in_buffer_size))
}

pub fn delete(junction: &Path) -> io::Result<()> {
    let handle =
        self::helpers::open_reparse_point(junction, winnt::GENERIC_READ | winnt::GENERIC_WRITE)?;
    self::helpers::delete_reparse_point(*handle)
}

pub fn exists(junction: &Path) -> io::Result<bool> {
    if !junction.exists() {
        return Ok(false);
    }
    let handle = self::helpers::open_reparse_point(junction, winnt::GENERIC_READ)?;
    // Allocate enough space to fit the maximum sized reparse data buffer
    let mut data = [0u8; winnt::MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize];
    // RedefKine the above char array into a ReparseDataBuffer we can work with
    let rdb = self::helpers::get_reparse_data_point(*handle, &mut data)?;
    // The reparse tag indicates if this is a junction or not
    Ok(rdb.reparse_tag == winnt::IO_REPARSE_TAG_MOUNT_POINT)
}

pub fn get_target(junction: &Path) -> io::Result<PathBuf> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, slice};
    if !junction.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "`junction` does not exist",
        ));
    }
    let handle = self::helpers::open_reparse_point(junction, winnt::GENERIC_READ)?;
    let mut data = [0u8; winnt::MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize];
    // RedefKine the above char array into a ReparseDataBuffer we can work with
    let rdb = self::helpers::get_reparse_data_point(*handle, &mut data)?;
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
