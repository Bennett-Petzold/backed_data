fn main() {
    cfg_aliases::cfg_aliases! {
        runtime: { any(feature = "tokio", feature = "smol") },
        mmap_impl: { all(feature = "mmap", not(target_os = "windows")) },
        unsafe_feature: { any(feature = "test", feature = "unsafe_array", feature = "tokio", feature = "smol", feature = "mmap", feature = "encrypted", feature = "network", feature = "async") },
    }
}
