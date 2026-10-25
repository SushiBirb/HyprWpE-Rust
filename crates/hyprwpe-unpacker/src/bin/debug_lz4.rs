use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let p = Path::new(&args[1]);
        if p.is_dir() {
            hyprwpe_unpacker::process_tex_files(p);
            println!("process_tex_files({:?}) completed", p);
        } else {
            let ok = hyprwpe_unpacker::process_single_tex(p);
            println!("process_single_tex({:?}) -> {}", p, ok);
        }
    }
}
