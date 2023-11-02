fn main() {
    #[cfg(all(feature = "std", feature = "serde"))]
    println!("cargo:rustc-cfg=feature=\"serde/std\"");
    #[cfg(all(feature = "alloc", feature = "serde"))]
    println!("cargo:rustc-cfg=feature=\"serde/alloc\"");
}
