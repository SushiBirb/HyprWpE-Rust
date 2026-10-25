import sys
import os
import json
import zipfile
import subprocess
from pathlib import Path

import struct
from PIL import Image

def decompress_lz4_block(src, uncompressed_size):
    dst = bytearray()
    src_len = len(src)
    i = 0
    while i < src_len:
        token = src[i]
        i += 1
        lit_len = token >> 4
        if lit_len == 15:
            while i < src_len:
                s = src[i]
                i += 1
                lit_len += s
                if s != 255:
                    break
        dst.extend(src[i:i+lit_len])
        i += lit_len
        if i >= src_len or len(dst) >= uncompressed_size:
            break
        offset = src[i] | (src[i+1] << 8)
        i += 2
        match_len = token & 0x0F
        if match_len == 15:
            while i < src_len:
                s = src[i]
                i += 1
                match_len += s
                if s != 255:
                    break
        match_len += 4
        for _ in range(match_len):
            dst.append(dst[-offset])
    return bytes(dst)

def process_tex_files(output_dir):
    """
    Scans output_dir for .tex files and extracts embedded PNGs, LZ4 compressed masks,
    or generates fallback textures so WebGL runtime texture loading never 404s.
    """
    for root, _, files in os.walk(output_dir):
        for file in files:
            if file.endswith('.tex'):
                tex_path = os.path.join(root, file)
                png_path = os.path.splitext(tex_path)[0] + '.png'
                if os.path.exists(png_path):
                    continue
                saved = False
                try:
                    with open(tex_path, 'rb') as f:
                        data = f.read()
                    idx = data.find(b'\x89PNG')
                    if idx != -1:
                        with open(png_path, 'wb') as f_out:
                            f_out.write(data[idx:])
                        saved = True
                    else:
                        texb_idx = data.find(b'TEXB')
                        if texb_idx != -1 and len(data) >= texb_idx + 41:
                            try:
                                width = struct.unpack('<I', data[texb_idx+21:texb_idx+25])[0]
                                height = struct.unpack('<I', data[texb_idx+25:texb_idx+29])[0]
                                uncomp_sz = struct.unpack('<I', data[texb_idx+33:texb_idx+37])[0]
                                comp_sz = struct.unpack('<I', data[texb_idx+37:texb_idx+41])[0]
                                if width > 0 and height > 0 and comp_sz > 0 and texb_idx + 41 + comp_sz <= len(data):
                                    raw_lz4 = data[texb_idx+41:texb_idx+41+comp_sz]
                                    decomp = decompress_lz4_block(raw_lz4, uncomp_sz)
                                    if len(decomp) >= width * height * 4:
                                        img = Image.frombytes('RGBA', (width, height), decomp[:width*height*4])
                                        img.save(png_path)
                                        saved = True
                                    elif len(decomp) >= width * height * 2:
                                        rg = decomp[:width*height*2]
                                        rgba = bytearray(width * height * 4)
                                        rgba[0::4] = rg[0::2]
                                        rgba[1::4] = rg[1::2]
                                        rgba[2::4] = b'\x80' * (width * height)
                                        rgba[3::4] = b'\xff' * (width * height)
                                        img = Image.frombytes('RGBA', (width, height), bytes(rgba))
                                        img.save(png_path)
                                        saved = True
                                    elif len(decomp) >= width * height:
                                        img = Image.frombytes('L', (width, height), decomp[:width*height])
                                        img.save(png_path)
                                        saved = True
                            except Exception:
                                pass
                except Exception as e:
                    print(f"Error processing {file}: {e}")

                if not saved:
                    try:
                        width, height = 1920, 1080
                        img = Image.new('RGBA', (width, height))
                        pixels = img.load()
                        for y in range(height):
                            r = int(30 + (y / height) * 70)
                            g = int(70 + (y / height) * 90)
                            b = int(140 + (y / height) * 100)
                            for x in range(0, width, 2):
                                pixels[x, y] = (r, g, b, 255)
                        img.save(png_path)
                    except Exception:
                        pass

def native_unpack_pkg(pkg_path, output_dir):
    """Native Python PKG unpacker supporting PKGV0001 through PKGV0024."""
    os.makedirs(output_dir, exist_ok=True)
    try:
        with open(pkg_path, 'rb') as f:
            magic_len = struct.unpack('<I', f.read(4))[0]
            magic = f.read(magic_len).decode('utf-8', errors='ignore')
            if not magic.startswith('PKGV'):
                return False
            file_count = struct.unpack('<I', f.read(4))[0]
            files = []
            for _ in range(file_count):
                name_len = struct.unpack('<I', f.read(4))[0]
                name = f.read(name_len).decode('utf-8', errors='ignore').rstrip('\x00')
                offset = struct.unpack('<I', f.read(4))[0]
                size = struct.unpack('<I', f.read(4))[0]
                files.append((name, offset, size))
            header_end = f.tell()

            for name, offset, size in files:
                target_path = os.path.join(output_dir, name.replace('\\', '/'))
                os.makedirs(os.path.dirname(target_path), exist_ok=True)
                f.seek(header_end + offset)
                data = f.read(size)
                with open(target_path, 'wb') as out_f:
                    out_f.write(data)
        return True
    except Exception as e:
        print(f"Native Python PKG unpack failed for {pkg_path}: {e}")
        return False

def unpack_pkg(pkg_path, output_dir):
    """
    Unpacks a Wallpaper Engine .pkg file and returns the project.json content if available.
    """
    if not os.path.exists(pkg_path):
        print(f"Error: File not found at {pkg_path}")
        return None

    os.makedirs(output_dir, exist_ok=True)
    extracted = native_unpack_pkg(pkg_path, output_dir)
    if extracted:
        print(f"Successfully extracted with native Python unpacker to {output_dir}")

    # Fallback to depkg if native unpacker failed
    if not extracted:
        depkg_bin = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'depkg')
        if os.path.exists(depkg_bin):
            try:
                print(f"Trying depkg extraction on {pkg_path}...")
                cmd = [depkg_bin, '-p', pkg_path, '-t', output_dir]
                result = subprocess.run(cmd, capture_output=True, text=True)
                if result.returncode == 0 and len(os.listdir(output_dir)) > 0:
                    print(f"Successfully extracted with depkg to {output_dir}")
                    extracted = True
            except Exception as e:
                print(f"Exception running depkg: {e}")

    # Try RePKG extraction second via dotnet
    if not extracted:
        repkg_exe = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'RePKG.exe')
        if os.path.exists(repkg_exe):
            try:
                print(f"Trying RePKG.exe extraction on {pkg_path}...")
                cmd = ['dotnet', repkg_exe, 'extract', pkg_path, '-o', output_dir, '--overwrite']
                result = subprocess.run(cmd, capture_output=True, text=True)
                if result.returncode == 0:
                    print(f"Successfully extracted with RePKG.exe to {output_dir}")
                    extracted = True
                else:
                    print(f"RePKG.exe failed with exit code {result.returncode}:\n{result.stderr}\n{result.stdout}")
                    print("Falling back to zipfile...")
            except Exception as e:
                print(f"Exception running RePKG.exe: {e}")
    
    # Fallback to ZipFile
    if not extracted:
        try:
            with zipfile.ZipFile(pkg_path, 'r') as zip_ref:
                zip_ref.extractall(output_dir)
                print(f"Successfully extracted with zipfile to {output_dir}")
                extracted = True
        except zipfile.BadZipFile:
            print(f"Error: {pkg_path} is not a valid zip file or is corrupted.")
        except Exception as e:
            print(f"An error occurred during zipfile extraction: {e}")
            
    if not extracted:
        return None

    # Automatically extract embedded PNGs from TEX files
    process_tex_files(output_dir)

    # Find and parse project.json
    project_json_path = os.path.join(output_dir, 'project.json')
    if os.path.exists(project_json_path):
        with open(project_json_path, 'r') as f:
            return json.load(f)
    else:
        print("Note: project.json not found in the package (normal for scenes).")
        return {}

if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("Usage: python unpacker.py <path_to_pkg_file> <output_directory>")
        sys.exit(1)
    
    pkg_file = sys.argv[1]
    out_dir = sys.argv[2]
    
    project_data = unpack_pkg(pkg_file, out_dir)
    
    if project_data is not None:
        if project_data:
            print("\nProject Details:")
            print(json.dumps(project_data, indent=2))
        else:
            print("\nExtraction completed (no project.json inside).")
