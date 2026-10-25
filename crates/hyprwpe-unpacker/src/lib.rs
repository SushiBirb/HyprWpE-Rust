pub mod lz4_decoder;
pub mod pkg_unpacker;
pub mod tex_processor;

pub use lz4_decoder::decompress_lz4_block;
pub use pkg_unpacker::{native_unpack_pkg, unpack_pkg, UnpackError};
pub use tex_processor::{process_single_tex, process_tex_files};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_lz4_decoder_empty_and_literal() {
        let input = vec![0x10, b'A']; // 1 literal 'A'
        let decompressed = decompress_lz4_block(&input, 1);
        assert_eq!(decompressed, b"A");
    }

    #[test]
    fn test_tex_fallback_generator() {
        let dir = tempdir().unwrap();
        let tex_path = dir.path().join("dummy.tex");
        fs::write(&tex_path, b"INVALID_TEXTURE_HEADER_TEST").unwrap();

        let success = process_single_tex(&tex_path);
        assert!(success);

        let png_path = dir.path().join("dummy.png");
        assert!(png_path.exists());
        assert!(fs::metadata(&png_path).unwrap().len() > 0);
    }
}
