use super::c::{MAXIMUM_REPARSE_DATA_BUFFER_SIZE, REPARSE_DATA_BUFFER};

type MaybeU8 = std::mem::MaybeUninit<u8>;

#[repr(align(4))]
pub struct BytesAsReparseDataBuffer {
    value: Box<[MaybeU8]>,
}

// Asserts that pointers of `BytesAsReparseDataBuffer` can be casted to
// `REPARSE_DATA_BUFFER`.
const _: () = {
    let a = align_of::<BytesAsReparseDataBuffer>();
    let b = align_of::<REPARSE_DATA_BUFFER>();
    assert!((a % b) == 0);
};

impl BytesAsReparseDataBuffer {
    pub fn new() -> Self {
        let boxed = Box::<[u8]>::new_uninit_slice(MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize);
        Self { value: boxed }
    }

    pub fn as_mut_ptr(&mut self) -> *mut REPARSE_DATA_BUFFER {
        self.value.as_mut_ptr().cast::<REPARSE_DATA_BUFFER>()
    }

    // FIXME: `MaybeUninit::assume_init` recv `self` ?
    pub unsafe fn assume_init(&mut self) -> &REPARSE_DATA_BUFFER {
        unsafe { &*self.as_mut_ptr() }
    }
}
