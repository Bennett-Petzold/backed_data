fn main() {
    cfg_aliases::cfg_aliases! {
        runtime: { any(feature = "tokio", feature = "smol") },
        mmap_impl: { all(feature = "mmap", not(target_os = "windows")) }
    }
}
