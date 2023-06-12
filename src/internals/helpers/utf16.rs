// FIXME(const_generic)
/// Convert ASCII bytes to UTF-16 sequences.
macro_rules! utf16s {
    ($src:literal) => {{
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

const fn ascii_to_utf16<const N: usize>(src: [u8; N]) -> [u16; N] {
    let dst = [0u16; N];
    let mut i = 0;
    while i < N {
        dst[i] = src[i] as u16;
        i += 1;
    }
    dst
}

#[test]
fn const_fn() {
    const _: [u16; 9] = ascii_to_utf16(br"123412341");
}

pub(crate) use utf16s;
