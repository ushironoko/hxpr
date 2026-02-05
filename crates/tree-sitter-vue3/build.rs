fn main() {
    let src_dir = std::path::Path::new("src");

    let mut cpp_config = cc::Build::new();
    cpp_config.cpp(true).include(src_dir);

    #[cfg(target_env = "msvc")]
    cpp_config.flag("-utf-8");

    // Use C++11 for scanner.cc (uses nullptr, etc.)
    #[cfg(not(target_env = "msvc"))]
    cpp_config.flag("-std=c++11");

    // parser.c needs C compiler
    let mut c_config = cc::Build::new();
    c_config.std("c11").include(src_dir);

    #[cfg(target_env = "msvc")]
    c_config.flag("-utf-8");

    let parser_path = src_dir.join("parser.c");
    c_config.file(&parser_path);
    println!("cargo:rerun-if-changed={}", parser_path.to_str().unwrap());

    // scanner.cc is C++
    let scanner_path = src_dir.join("scanner.cc");
    if scanner_path.exists() {
        cpp_config.file(&scanner_path);
        println!("cargo:rerun-if-changed={}", scanner_path.to_str().unwrap());
    }

    // Compile C and C++ separately
    c_config.compile("tree-sitter-vue3-parser");
    cpp_config.compile("tree-sitter-vue3-scanner");
}
