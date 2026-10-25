/// Decompress LZ4 raw block using lz4_flex crate
pub fn decompress_lz4_block(src: &[u8], uncompressed_size: usize) -> Vec<u8> {
    if let Ok(decomp) = lz4_flex::decompress(src, uncompressed_size) {
        return decomp;
    }

    // Fallback: custom raw block loop
    let cap = uncompressed_size.min(50 * 1024 * 1024);
    let mut dst = Vec::with_capacity(cap);
    let src_len = src.len();
    let mut i = 0;

    while i < src_len {
        let token = src[i];
        i += 1;

        let mut lit_len = (token >> 4) as usize;
        if lit_len == 15 {
            while i < src_len {
                let s = src[i] as usize;
                i += 1;
                lit_len += s;
                if s != 255 {
                    break;
                }
            }
        }

        if i + lit_len <= src_len {
            dst.extend_from_slice(&src[i..i + lit_len]);
        } else {
            dst.extend_from_slice(&src[i..]);
        }
        i += lit_len;

        if i >= src_len || dst.len() >= uncompressed_size {
            break;
        }

        if i + 2 > src_len {
            break;
        }
        let offset = (src[i] as usize) | ((src[i + 1] as usize) << 8);
        i += 2;

        if offset == 0 || offset > dst.len() {
            break;
        }

        let mut match_len = (token & 0x0F) as usize;
        if match_len == 15 {
            while i < src_len {
                let s = src[i] as usize;
                i += 1;
                match_len += s;
                if s != 255 {
                    break;
                }
            }
        }
        match_len += 4;

        for _ in 0..match_len {
            let back_idx = dst.len() - offset;
            let val = dst[back_idx];
            dst.push(val);
        }
    }

    dst
}
