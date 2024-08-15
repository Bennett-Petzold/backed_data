![Tests][Tests]
![Pages][Pages]
![Coverage][Coverage]

Build docs with `RUSTDOCFLAGS='--cfg docsrs' cargo +nightly doc --open --all-features --no-deps`.

Build zstd-x-lto flags with `RUSTFLAGS="-C linker-plugin-lto -C linker=clang -C link-arg=-fuse-ld=lld" CC=clang cargo build`.
This builds everything on the same compiler and helps the linker get its life together.
Run the regular tests, but not the doc tests, when built with these features.
Cargo, rustdoc, and external lto do not all mix properly.

Run tests including doc tests with `cargo test --features zstdmt,async-zstdmt`.

Test coverage with `cargo tarpaulin --features zstdmt,async-zstdmt --exclude-files '*/lib/*' build.rs`.

When testing with Miri, use 'MIRIFLAGS="-Zmiri-disable-isolation"'.

[Tests]: https://github.com/Bennett-Petzold/backed_data//actions/workflows/all-tests.yml/badge.svg?branch=main
[Pages]: https://github.com/Bennett-Petzold/backed_data//actions/workflows/pages.yml/badge.svg?branch=main
[Coverage]: https://github.com/Bennett-Petzold/backed_data//actions/workflows/all-tests.yml/badge.svg?branch=main
