use std::process::Command;

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let shaders = &["basic", "light"];

    for shader in shaders {
        let shader_path = format!("src/shaders/{shader}.nzsl");
        let output = Command::new("../nzslc")
            .args(&[
                &format!("--output={out_dir}"),
                &shader_path,
                "--compile=spv",
                "--module=src/shaders",
                "--optimize",
            ])
            .output()
            .expect("failed to execute nzslc");


        if !output.status.success() {
            eprintln!("{}", std::str::from_utf8(&output.stderr).unwrap());
            panic!("failed to compile shaders");
        }
    }

    println!("cargo:rerun-if-changed=src/shaders");
}
