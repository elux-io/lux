use std::process::Command;

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();

    let output = Command::new("../nzslc")
        .args(&[
            "src/shader.nzsl",
            &format!("--output={}", out_dir),
            "--compile=spv",
        ])
        .output()
        .expect("failed to execute nzslc");

    if !output.status.success() {
        eprintln!("{}", std::str::from_utf8(&output.stderr).unwrap());
        panic!("failed to compile shaders");
    }

    println!("cargo:rerun-if-changed=src/shader.nzsl");
}
