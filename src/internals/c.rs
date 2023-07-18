#![allow(non_snake_case)]

use std::alloc::Layout;
use std::mem::size_of;
use std::os::raw::{c_ulong, c_ushort};
use std::os::windows::io::RawHandle;

use windows_sys::core::GUID;
pub use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, SetLastError, ERROR_INSUFFICIENT_BUFFER, FALSE, GENERIC_READ, GENERIC_WRITE, HANDLE,
    INVALID_HANDLE_VALUE,
};
pub use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES,
};
// See more in <https://learn.microsoft.com/en-us/windows/win32/secauthz/privilege-constants>.
pub use windows_sys::Win32::Security::{SE_BACKUP_NAME, SE_CREATE_SYMBOLIC_LINK_NAME, SE_RESTORE_NAME};
pub use windows_sys::Win32::Storage::FileSystem::{
    GetFullPathNameW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, MAXIMUM_REPARSE_DATA_BUFFER_SIZE,
    REPARSE_GUID_DATA_BUFFER,
};
pub use windows_sys::Win32::System::Ioctl::{
    FSCTL_DELETE_REPARSE_POINT, FSCTL_GET_REPARSE_POINT, FSCTL_SET_REPARSE_POINT,
};
pub use windows_sys::Win32::System::SystemServices::IO_REPARSE_TAG_MOUNT_POINT;
pub use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
pub use windows_sys::Win32::System::IO::DeviceIoControl;

// Makes sure layout of RawHandle and windows-sys's HANDLE are the same
// for pointer casts between them.
// CLIPPY: nonsense suggestions for assert!
#[allow(clippy::unnecessary_operation)]
const _: () = {
    let std_layout = Layout::new::<RawHandle>();
    let win_sys_layout = Layout::new::<HANDLE>();
    // MSVR(Rust v1.57): use assert! instead
    [(); 1][!(std_layout.size() == win_sys_layout.size()) as usize];
    [(); 1][!(std_layout.align() == win_sys_layout.align()) as usize];
};

// NOTE: to use `size_of` operator, below structs should be packed.
// TODO: use `offset_of!` when stabilized.
/// Reparse Data Buffer header size
pub const REPARSE_DATA_BUFFER_HEADER_SIZE: u16 = 8;
/// Reparse GUID Data Buffer header size
pub const REPARSE_GUID_DATA_BUFFER_HEADER_SIZE: u16 = 24;
/// MountPointReparseBuffer header size
pub const MOUNT_POINT_REPARSE_BUFFER_HEADER_SIZE: u16 = 8;

// Safety checks for correct header size due to the lacks of `offset_of!`.
// MSRV(1.57): for const assert!
#[allow(clippy::no_effect)] // noisy clippy lints
const _: () = {
    let rdb_header_size = size_of::<c_ulong>() + size_of::<c_ushort>() * 2;
    [(); 1][!(rdb_header_size == REPARSE_DATA_BUFFER_HEADER_SIZE as _) as usize];

    let mprb_header_size = size_of::<c_ushort>() * 4;
    [(); 1][!(mprb_header_size == MOUNT_POINT_REPARSE_BUFFER_HEADER_SIZE as _) as usize];

    let rgdb_header_size = size_of::<c_ulong>() + size_of::<c_ushort>() * 2 + size_of::<GUID>();
    [(); 1][!(rgdb_header_size == REPARSE_GUID_DATA_BUFFER_HEADER_SIZE as _) as usize];
};

type VarLenArr<T> = [T; 1];

/// This structure contains reparse point data for a Microsoft reparse point.
///
/// Read more:
/// * https://msdn.microsoft.com/en-us/windows/desktop/ff552012
/// * https://www.pinvoke.net/default.aspx/Structures.REPARSE_DATA_BUFFER
#[repr(C)]
#[derive(Debug)]
pub struct REPARSE_DATA_BUFFER {
    /// Reparse point tag. Must be a Microsoft reparse point tag.
    pub ReparseTag: c_ulong,
    // Size, in bytes, of the data after the Reserved member.
    // This can be calculated by:
    // MOUNT_POINT_REPARSE_BUFFER_HEADER_SIZE + SubstituteNameLength
    // + PrintNameLength + (names.nul_terminated() ? 2 * sizeof(char) : 0);
    pub ReparseDataLength: c_ushort,
    /// Reversed. It SHOULD be set to 0, and MUST be ignored.
    pub Reserved: c_ushort,
    pub ReparseBuffer: MountPointReparseBuffer,
}

#[repr(C)]
#[derive(Debug)]
pub struct MountPointReparseBuffer {
    /// Offset, in bytes, of the substitute name string in the `PathBuffer` array.
    /// Note that this offset must be divided by `sizeof(u16)` to get the array index.
    pub SubstituteNameOffset: c_ushort,
    /// Length, in bytes, of the substitute name string. If this string is `NULL`-terminated,
    /// it does not include space for the `UNICODE_NULL` character.
    pub SubstituteNameLength: c_ushort,
    /// Offset, in bytes, of the print name string in the `PathBuffer` array.
    /// Note that this offset must be divided by `sizeof(u16)` to get the array index.
    pub PrintNameOffset: c_ushort,
    /// Length, in bytes, of the print name string. If this string is `NULL`-terminated,
    /// it does not include space for the `UNICODE_NULL` character.
    pub PrintNameLength: c_ushort,
    /// A buffer containing the Unicode-encoded path string. The path string contains the
    /// substitute name string and print name string. The substitute name and print name strings
    /// can appear in any order in the PathBuffer. (To locate the substitute name and print name
    /// strings in the PathBuffer, use the `SubstituteNameOffset`, `SubstituteNameLength`,
    /// `PrintNameOffset`, and `PrintNameLength` members.)
    pub PathBuffer: VarLenArr<c_ushort>,
}
