fn main() {
    // The chip cfgs esp-idf-sys passes on (esp32, esp32s3...).
    println!("cargo::rustc-check-cfg=cfg(esp32, esp32s2, esp32s3, esp32c3)");
    embuild::espidf::sysenv::output();
}
