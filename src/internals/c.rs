use std::alloc::Layout;
use std::mem::transmute;
use std::os::windows::io::RawHandle;

pub use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, SetLastError, GENERIC_READ, GENERIC_WRITE, HANDLE,
};
pub use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES,
};
pub use windows_sys::Win32::Storage::FileSystem::{
    GetFullPathNameW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, MAXIMUM_REPARSE_DATA_BUFFER_SIZE,
};
pub use windows_sys::Win32::System::Ioctl::{
    FSCTL_DELETE_REPARSE_POINT, FSCTL_GET_REPARSE_POINT, FSCTL_SET_REPARSE_POINT,
};
pub use windows_sys::Win32::System::SystemServices::IO_REPARSE_TAG_MOUNT_POINT;
pub use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
pub use windows_sys::Win32::System::IO::DeviceIoControl;

pub fn same_handle(h: RawHandle) -> HANDLE {
    // Makes sure layout of RawHandle and windows-sys's HANDLE are the same
    // for pointer casts between them.
    // CLIPPY: nonsense suggestions for assert!
    #[allow(clippy::unnecessary_operation)]
    const _: () = {
        let std_layout = Layout::new::<RawHandle>();
        let winapi_layout = Layout::new::<HANDLE>();
        // MSVR(Rust v1.57): use assert! instead
        [(); 1][!(std_layout.size() == winapi_layout.size()) as usize];
        [(); 1][!(std_layout.align() == winapi_layout.align()) as usize];
    };

    // SAFETY: assured by above comparisons
    unsafe { transmute::<RawHandle, HANDLE>(h) }
}
