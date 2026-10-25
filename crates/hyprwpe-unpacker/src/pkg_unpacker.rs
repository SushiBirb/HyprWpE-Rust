use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::Command;
use byteorder::{LittleEndian, ReadBytesExt};
use serde_json::Value;
use thiserror::Error;

use crate::tex_processor::process_tex_files;

#[derive(Debug, Error)]
pub enum UnpackError {
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid PKG header")]
    InvalidHeader,
    #[error("Unpacking failed for package")]
    ExtractionFailed,
}

pub fn native_unpack_pkg(pkg_path: &Path, output_dir: &Path) -> Result<bool, UnpackError> {
    if !pkg_path.exists() {
        return Err(UnpackError::FileNotFound(pkg_path.to_string_lossy().to_string()));
    }

    fs::create_dir_all(output_dir)?;

    let mut file = File::open(pkg_path)?;
    let magic_len = match file.read_u32::<LittleEndian>() {
        Ok(len) => len as usize,
        Err(_) => return Ok(false),
    };

    if magic_len > 256 {
        return Ok(false);
    }

    let mut magic_bytes = vec![0u8; magic_len];
    if file.read_exact(&mut magic_bytes).is_err() {
        return Ok(false);
    }

    let magic = String::from_utf8_lossy(&magic_bytes);
    if !magic.starts_with("PKGV") {
        return Ok(false);
    }

    let file_count = match file.read_u32::<LittleEndian>() {
        Ok(cnt) => cnt as usize,
        Err(_) => return Ok(false),
    };

    struct FileEntry {
        name: String,
        offset: u64,
        size: u64,
    }

    let mut files = Vec::with_capacity(file_count);
    for _ in 0..file_count {
        let name_len = match file.read_u32::<LittleEndian>() {
            Ok(len) => len as usize,
            Err(_) => return Ok(false),
        };

        if name_len > 1024 {
            return Ok(false);
        }

        let mut name_bytes = vec![0u8; name_len];
        if file.read_exact(&mut name_bytes).is_err() {
            return Ok(false);
        }

        let raw_name = String::from_utf8_lossy(&name_bytes);
        let name = raw_name.trim_matches('\0').replace('\\', "/");
        let clean_name = name.trim_start_matches('/').to_string();

        let offset = match file.read_u32::<LittleEndian>() {
            Ok(off) => off as u64,
            Err(_) => return Ok(false),
        };

        let size = match file.read_u32::<LittleEndian>() {
            Ok(sz) => sz as u64,
            Err(_) => return Ok(false),
        };

        files.push(FileEntry { name: clean_name, offset, size });
    }

    let header_end = file.stream_position()?;

    for entry in files {
        let target_path = output_dir.join(&entry.name);
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if entry.size > 500 * 1024 * 1024 {
            // Guard against corrupt or invalid entry sizes > 500MB
            continue;
        }

        if file.seek(SeekFrom::Start(header_end + entry.offset)).is_ok() {
            let mut take_handle = (&mut file).take(entry.size);
            if let Ok(mut out_f) = File::create(&target_path) {
                let _ = std::io::copy(&mut take_handle, &mut out_f);
            }
        }
    }

    Ok(true)
}

pub fn unpack_pkg(pkg_path: &Path, output_dir: &Path) -> Option<Value> {
    if !pkg_path.exists() {
        return None;
    }

    let _ = fs::create_dir_all(output_dir);
    let mut extracted = native_unpack_pkg(pkg_path, output_dir).unwrap_or(false);

    // Fallback 1: depkg binary
    if !extracted {
        let current_exe = std::env::current_exe().unwrap_or_default();
        let crate_dir = current_exe.parent().unwrap_or(Path::new("."));
        let depkg_bin = crate_dir.join("depkg");

        if depkg_bin.exists() {
            let output = Command::new(&depkg_bin)
                .arg("-p")
                .arg(pkg_path)
                .arg("-t")
                .arg(output_dir)
                .output();

            if let Ok(out) = output {
                if out.status.success() {
                    extracted = true;
                }
            }
        }
    }

    // Fallback 2: RePKG.exe via dotnet
    if !extracted {
        let current_exe = std::env::current_exe().unwrap_or_default();
        let crate_dir = current_exe.parent().unwrap_or(Path::new("."));
        let repkg_exe = crate_dir.join("RePKG.exe");

        if repkg_exe.exists() {
            let output = Command::new("dotnet")
                .arg(&repkg_exe)
                .arg("extract")
                .arg(pkg_path)
                .arg("-o")
                .arg(output_dir)
                .arg("--overwrite")
                .output();

            if let Ok(out) = output {
                if out.status.success() {
                    extracted = true;
                }
            }
        }
    }

    // Fallback 3: zip archive
    if !extracted {
        if let Ok(file) = File::open(pkg_path) {
            if let Ok(mut archive) = zip::ZipArchive::new(file) {
                if archive.extract(output_dir).is_ok() {
                    extracted = true;
                }
            }
        }
    }

    if !extracted {
        return None;
    }

    process_tex_files(output_dir);

    let project_json_path = output_dir.join("project.json");
    if project_json_path.exists() {
        if let Ok(content) = fs::read_to_string(&project_json_path) {
            if let Ok(val) = serde_json::from_str::<Value>(&content) {
                return Some(val);
            }
        }
    }

    Some(Value::Object(serde_json::Map::new()))
}
