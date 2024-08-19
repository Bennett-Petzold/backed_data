[![Crate][CrateStatus]][Crate]
[![Tests][TestsStatus]][Tests]
[![Docs][PagesStatus]][Docs]
[![Coverage][Coverage]][CoveragePages]

Cache data outside memory, loading in when referenced.

See the [documentation][Docs] for more detail.

# Development Commands

When testing with Miri, use 'MIRIFLAGS="-Zmiri-disable-isolation"' to allow I/O.
Build docs with `RUSTDOCFLAGS='--cfg docsrs' cargo +nightly doc --open --all-features --no-deps`.

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
* Ensure `adapters` has no race conditions that can cause program hang.
* Consider refactoring WriteDisk to remove the need for runtime borrow checks in `mmap`.
* Implement missing standard library traits (e.g. `[]` access).
* Add array iterators that minimize memory usage by dropping backing stores when no longer borrowed.

# General TODO
* Add support for more disk formats and encoders

[CrateStatus]: https://img.shields.io/crates/v/backed_data.svg
[Crate]: https://crates.io/crates/backed_data
[TestsStatus]: https://github.com/Bennett-Petzold/backed_data/actions/workflows/all-tests.yml/badge.svg?branch=main
[Tests]: https://github.com/Bennett-Petzold/backed_data/actions/workflows/all-tests.yml
[PagesStatus]: https://github.com/Bennett-Petzold/backed_data/actions/workflows/pages.yml/badge.svg?branch=main
[Docs]: https://bennett-petzold.github.io/backed_data/docs/backed_data/
[Coverage]: https://bennett-petzold.github.io/backed_data/coverage/badge.svg
[CoveragePages]: https://bennett-petzold.github.io/backed_data/coverage/
