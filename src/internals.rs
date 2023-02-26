mod helpers;
mod types;

use types::ReparseDataBuffer;
use types::{MOUNT_POINT_REPARSE_BUFFER_HEADER_SIZE, REPARSE_DATA_BUFFER_HEADER_SIZE};

use std::cmp;
use std::fs;
use std::path::{Path, PathBuf};
use std::ptr;
use std::slice;
use std::{ffi::OsString, os::windows::ffi::OsStringExt};
use std::{io, os::windows::io::AsRawHandle};

use winapi::um::winnt::{IO_REPARSE_TAG_MOUNT_POINT, MAXIMUM_REPARSE_DATA_BUFFER_SIZE};

// makes sure layout of RawHandle and winapi's HANDLE are the same
// for pointer casts between them.
const _: () = {
    use std::alloc::Layout;
    let std_layout = Layout::new::<std::os::windows::io::RawHandle>();
    let winapi_layout = Layout::new::<winapi::um::winnt::HANDLE>();
    // MSVR(Rust v1.57): use assert! instead
    [(); 1][!(std_layout.size() == winapi_layout.size()) as usize];
    [(); 1][!(std_layout.align() == winapi_layout.align()) as usize];
};

/// This prefix indicates to NTFS that the path is to be treated as a non-interpreted
/// path in the virtual file system.
const NON_INTERPRETED_PATH_PREFIX: [u16; 4] = [b'\\' as u16, b'?' as _, b'?' as _, b'\\' as _];
const WCHAR_SIZE: u16 = std::mem::size_of::<u16>() as _;

pub fn create(target: &Path, junction: &Path) -> io::Result<()> {
    const UNICODE_NULL_SIZE: u16 = WCHAR_SIZE;
    const MAX_AVAILABLE_PATH_BUFFER: u16 = MAXIMUM_REPARSE_DATA_BUFFER_SIZE as u16
        - REPARSE_DATA_BUFFER_HEADER_SIZE
        - MOUNT_POINT_REPARSE_BUFFER_HEADER_SIZE
        - 2 * UNICODE_NULL_SIZE;

    // We're using low-level APIs to create the junction, and these are more picky about paths.
    // For example, forward slashes cannot be used as a path separator, so we should try to
    // canonicalize the path first.
    let mut target = helpers::get_full_path(target)?;
    fs::create_dir(junction)?;
    let file = helpers::open_reparse_point(junction, true)?;
    // "\??\" + target
    let len = NON_INTERPRETED_PATH_PREFIX.len().saturating_add(target.len());
    let target_len_in_bytes = {
        let min_len = cmp::min(len, u16::MAX as usize) as u16;
        // Len without `UNICODE_NULL` at the end
        let target_len_in_bytes = min_len.saturating_mul(WCHAR_SIZE);
        // Check if `target_wchar.len()` may lead to a buffer overflow.
        if target_len_in_bytes > MAX_AVAILABLE_PATH_BUFFER {
            return Err(io::Error::new(io::ErrorKind::Other, "`target` is too long"));
        }
        target_len_in_bytes
    };
    let mut target_wchar: Vec<u16> = Vec::with_capacity(len);
    target_wchar.extend(&NON_INTERPRETED_PATH_PREFIX);
    target_wchar.append(&mut target);

    // Redefine the above char array into a ReparseDataBuffer we can work with
    let mut data = AlignAs {
        value: Vec::with_capacity(MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize),
    };
    let rdb = data.value.as_mut_ptr().cast::<ReparseDataBuffer>();
    let in_buffer_size: u16 = unsafe {
        // Set the type of reparse point we are creating
        ptr::addr_of_mut!((*rdb).reparse_tag).write(IO_REPARSE_TAG_MOUNT_POINT);
        ptr::addr_of_mut!((*rdb).reserved).write(0);

        // Copy the junction's target
        ptr::addr_of_mut!((*rdb).reparse_buffer.substitute_name_offset).write(0);
        ptr::addr_of_mut!((*rdb).reparse_buffer.substitute_name_length).write(target_len_in_bytes);

        // Copy the junction's link name
        ptr::addr_of_mut!((*rdb).reparse_buffer.print_name_offset).write(target_len_in_bytes + UNICODE_NULL_SIZE);
        ptr::addr_of_mut!((*rdb).reparse_buffer.print_name_length).write(0);

        // Safe because we checked `MAX_AVAILABLE_PATH_BUFFER`
        ptr::copy_nonoverlapping(
            target_wchar.as_ptr().cast::<u16>(),
            ptr::addr_of_mut!((*rdb).reparse_buffer.path_buffer).cast(),
            target_wchar.len(),
        );

        // Set the total size of the data buffer
        let size = target_len_in_bytes.wrapping_add(MOUNT_POINT_REPARSE_BUFFER_HEADER_SIZE + 2 * UNICODE_NULL_SIZE);
        ptr::addr_of_mut!((*rdb).reparse_data_length).write(size);
        size.wrapping_add(REPARSE_DATA_BUFFER_HEADER_SIZE)
    };

    helpers::set_reparse_point(file.as_raw_handle().cast(), rdb, u32::from(in_buffer_size))
}

pub fn delete(junction: &Path) -> io::Result<()> {
    let file = helpers::open_reparse_point(junction, true)?;
    helpers::delete_reparse_point(file.as_raw_handle().cast())
}

// Makes sure `align(ReparseDataBuffer) == 4` for struct `AlignAs` to be sound.
const _: () = {
    const A: usize = std::mem::align_of::<ReparseDataBuffer>();
    if A != 4 {
        let _ = [0; 0][A];
    }
};

type MaybeU8 = std::mem::MaybeUninit<u8>;
#[repr(align(4))]
struct AlignAs {
    value: Vec<MaybeU8>,
}

pub fn exists(junction: &Path) -> io::Result<bool> {
    if !junction.exists() {
        return Ok(false);
    }
    let file = helpers::open_reparse_point(junction, false)?;
    // Allocate enough space to fit the maximum sized reparse data buffer
    let mut data = AlignAs {
        value: Vec::with_capacity(MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize),
    };
    let rdb = data.value.as_mut_ptr().cast::<ReparseDataBuffer>();
    helpers::get_reparse_data_point(file.as_raw_handle().cast(), rdb)?;
    // The reparse tag indicates if this is a junction or not
    Ok(unsafe { (*rdb).reparse_tag } == IO_REPARSE_TAG_MOUNT_POINT)
}

pub fn get_target(junction: &Path) -> io::Result<PathBuf> {
    if !junction.exists() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "`junction` does not exist"));
    }
    let file = helpers::open_reparse_point(junction, false)?;
    let mut data = AlignAs {
        value: Vec::with_capacity(MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize),
    };
    let rdb = data.value.as_mut_ptr().cast::<ReparseDataBuffer>();
    helpers::get_reparse_data_point(file.as_raw_handle().cast(), rdb)?;
    // SAFETY: rdb should be initialized now
    let rdb = unsafe { &*rdb };
    if rdb.reparse_tag == IO_REPARSE_TAG_MOUNT_POINT {
        let offset = rdb.reparse_buffer.substitute_name_offset / WCHAR_SIZE;
        let len = rdb.reparse_buffer.substitute_name_length / WCHAR_SIZE;
        let wide = unsafe {
            let buf = rdb.reparse_buffer.path_buffer.as_ptr().add(offset as usize);
            slice::from_raw_parts(buf, len as usize)
        };
        // In case of "\??\C:\foo\bar"
        let wide = wide.strip_prefix(&NON_INTERPRETED_PATH_PREFIX).unwrap_or(wide);
        Ok(PathBuf::from(OsString::from_wide(wide)))
    } else {
        Err(io::Error::new(io::ErrorKind::Other, "not a reparse tag mount point"))
    }
}
