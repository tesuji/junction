// FIXME(const_generic)
/// Convert ASCII bytes to UTF-16 sequences.
macro_rules! utf16s {
    ($src:expr) => {{
        const SRC: &[u8] = $src;
        const N: usize = SRC.len();
        let mut i = 0;
        let mut dst = [0u16; N];
        while i < N {
            dst[i] = SRC[i] as u16;
            i += 1;
        }
        dst
    }};
}
