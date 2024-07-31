fn main() {
    cfg_aliases::cfg_aliases! {
        runtime: { any(feature = "tokio", feature = "smol") }
    }
}
