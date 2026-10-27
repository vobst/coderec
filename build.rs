use std::env;
use std::fs;
use std::io::{BufReader, BufWriter, Read};
use std::path::{Path, PathBuf};

const CORPUS_DIR: &str = "cpu_rec_corpus";
const LFS_POINTER_PREFIX: &[u8] = b"version https://git-lfs.github.com/spec/v1";
const HINT: &str = "Make sure git-lfs is installed and run `git lfs install && git lfs pull`.";

/// The corpus is stored as xz-compressed files via git-lfs. Decompress them
/// into `$OUT_DIR/cpu_rec_corpus`, from where they are embedded into the
/// binary.
fn main() {
    println!("cargo:rerun-if-changed={CORPUS_DIR}");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap()).join(CORPUS_DIR);
    if out_dir.exists() {
        fs::remove_dir_all(&out_dir).unwrap();
    }
    fs::create_dir_all(&out_dir).unwrap();

    let entries = fs::read_dir(CORPUS_DIR)
        .unwrap_or_else(|e| panic!("Could not read corpus directory '{CORPUS_DIR}': {e}. {HINT}"));

    let mut corpus_files = 0;
    for entry in entries {
        let path = entry.unwrap().path();
        let Some(name) = path.file_name().unwrap().to_str().and_then(|n| n.strip_suffix(".xz")) else {
            continue;
        };
        corpus_files += 1;
        decompress(&path, &out_dir.join(name));
    }

    if corpus_files == 0 {
        panic!("No corpus files found in '{CORPUS_DIR}'. {HINT}");
    }
}

fn decompress(src: &Path, dst: &Path) {
    // Without git-lfs, the checkout only contains small pointer files.
    let mut prefix = [0u8; LFS_POINTER_PREFIX.len()];
    let mut file = fs::File::open(src).unwrap();
    if file.read_exact(&mut prefix).is_ok() && prefix == LFS_POINTER_PREFIX {
        panic!("Corpus file '{}' is a git-lfs pointer. {HINT}", src.display());
    }

    let mut reader = BufReader::new(fs::File::open(src).unwrap());
    let mut writer = BufWriter::new(fs::File::create(dst).unwrap());
    lzma_rs::xz_decompress(&mut reader, &mut writer)
        .unwrap_or_else(|e| panic!("Could not decompress corpus file '{}': {e:?}", src.display()));
}
