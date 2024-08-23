[![Crate][CrateStatus]][Crate]
[![Tests][TestsStatus]][Tests]
[![Docs][PagesStatus]][Docs]
[![Coverage][Coverage]][CoveragePages]

Cache data outside memory, loading in when referenced.

This library provides a solution for large data that is sparsely accessed.
Ideally, it can minimize the amount of memory allocated by a program but never read.
It builds around `BackedEntry`, a struct that encodes and decodes data against a disk
(some type providing `Read` and `Write` handles). The data can then kept solely on disk when
not referenced to save memory space. The `BackedArray` and `DirectoryBackedArray` types
allow splitting datasets into multiple disks, reducing memory usage when significant
chunks are unused by program logic.

See the [documentation][Docs] for usage and more details.

<h3 align = "center"> Core BackedEntry Flow </h3>

![Backed Load Graphic][BackedLoad]

# Development Commands

When testing with Miri, use 'MIRIFLAGS="-Zmiri-disable-isolation"' to allow I/O.
Build docs with `RUSTDOCFLAGS='--cfg docsrs' cargo +nightly doc --open --all-features --no-deps`.

`media` has a crate to generate the documentation gifs. It requires `graphviz`
to create the frames and `gifsicle` to optimize for a drastic size decrease.
The gifs are stored in version control for linking into documentation; do not
overwrite the gifs in a commit unless another graphic was added.

# crates.io blocker
This relies on a fork of `secrets` (<https://github.com/stouset/secrets>) for a feature flag.
The repository does not seem to be actively maintained, and I have not had
success contacting the owner. The following options are under consideration:
* Continue waiting for contact from the maintainer
* Remove the `encrypted` feature
* Add the fork of `secrets` to this codebase
* Publish the fork of `secrets` to crates.io under another name

# TODO before 1.0 release
* Achieve full documentation of the API, including explanatory documentation.
    * Also ensure the documentation is reviewed to be accurate, clear, and brief.
* Ensure all uses of `unsafe` are necessary, well-described, and sound.
* Achieve reasonable functional coverage for the core API and all the disks/formats.
* Add descriptions to all examples, and refactor for clarity if necessary.
* Trim the `BackedArray` methods. 
* Implement missing standard library traits (e.g. `[]` access).
* Add array iterators that minimize memory usage by dropping backing stores when no longer borrowed.

# General TODO
* Add support for more disk formats and encoders
* Provide more detail in `alternatives.rs`
* Replace all async fn with Futures to avoid trait boxing

[CrateStatus]: https://img.shields.io/crates/v/backed_data.svg
[Crate]: https://crates.io/crates/backed_data
[TestsStatus]: https://github.com/Bennett-Petzold/backed_data/actions/workflows/all-tests.yml/badge.svg?branch=main
[Tests]: https://github.com/Bennett-Petzold/backed_data/actions/workflows/all-tests.yml
[PagesStatus]: https://github.com/Bennett-Petzold/backed_data/actions/workflows/pages.yml/badge.svg?branch=main
[Docs]: https://bennett-petzold.github.io/backed_data/docs/backed_data/
[Coverage]: https://bennett-petzold.github.io/backed_data/coverage/badge.svg
[CoveragePages]: https://bennett-petzold.github.io/backed_data/coverage/

[BackedLoad]: /media_output/backed_load.gif
