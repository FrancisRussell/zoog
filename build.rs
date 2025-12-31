use std::io::Write as _;
use std::path::PathBuf;
use std::time::Duration;
use std::{env, fs, io, thread};

fn main() {
    // Where build scripts are allowed to write
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let out_file = out_dir.join("generated.txt");

    // Write a file
    fs::write(&out_file, "hello from build.rs\n").unwrap();

    // Tell Cargo to rerun if this file changes
    println!("cargo:rerun-if-changed={}", out_file.display());

    // Normal stdout
    for i in 1..=10 {
        println!("stdout line {}", i);
    }

    // Stderr
    for i in 1..=10 {
        eprintln!("stderr line {}", i);
    }

    io::stdout().flush().unwrap();
    io::stderr().flush().unwrap();

    thread::sleep(Duration::from_secs(5));

    panic!("Oh no, I made a panic!");
}
