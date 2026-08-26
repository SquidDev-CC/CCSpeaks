use std::env;
use std::path::PathBuf;

fn main() {
  println!("cargo:rustc-link-lib=espeak-ng");

  let bindings = bindgen::Builder::default()
    .header("bindings.h")
    .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
    .rust_edition(bindgen::RustEdition::Edition2024)
    .wrap_unsafe_ops(true)
    .allowlist_item("^espeak.*")
    .default_enum_style(bindgen::EnumVariation::ModuleConsts)
    .generate()
    .expect("Unable to generate bindings");

  // Write the bindings to the $OUT_DIR/bindings.rs file.
  let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
  bindings
    .write_to_file(out_path.join("bindings.rs"))
    .expect("Couldn't write bindings!");
}
