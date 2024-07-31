Build docs with `RUSTDOCFLAGS='--cfg docsrs' cargo +nightly doc --open --all-features`.

Build zstd-x-lto flags with `RUSTFLAGS="-C linker-plugin-lto -C linker=clang -C link-arg=-fuse-ld=lld" CC=clang cargo build`.
This builds everything on the same compiler and helps the linker get its life together.

Test coverage with `RUSTFLAGS="-C linker-plugin-lto -C linker=clang -C link-arg=-fuse-ld=lld" CC=clang cargo tarpaulin --all-features --exclude-files '*/lib/*' build.rs`
