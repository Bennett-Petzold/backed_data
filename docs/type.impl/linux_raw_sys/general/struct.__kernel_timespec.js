(function() {
    var type_impls = Object.fromEntries([["rustix",[["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-Clone-for-__kernel_timespec\" class=\"impl\"><a class=\"src rightside\" href=\"src/linux_raw_sys/x86_64/general.rs.html#342\">source</a><a href=\"#impl-Clone-for-__kernel_timespec\" class=\"anchor\">§</a><h3 class=\"code-header\">impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html\" title=\"trait core::clone::Clone\">Clone</a> for <a class=\"struct\" href=\"linux_raw_sys/general/struct.__kernel_timespec.html\" title=\"struct linux_raw_sys::general::__kernel_timespec\">__kernel_timespec</a></h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.clone\" class=\"method trait-impl\"><a class=\"src rightside\" href=\"src/linux_raw_sys/x86_64/general.rs.html#342\">source</a><a href=\"#method.clone\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html#tymethod.clone\" class=\"fn\">clone</a>(&amp;self) -&gt; <a class=\"struct\" href=\"linux_raw_sys/general/struct.__kernel_timespec.html\" title=\"struct linux_raw_sys::general::__kernel_timespec\">__kernel_timespec</a></h4></section></summary><div class='docblock'>Returns a copy of the value. <a href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html#tymethod.clone\">Read more</a></div></details><details class=\"toggle method-toggle\" open><summary><section id=\"method.clone_from\" class=\"method trait-impl\"><span class=\"rightside\"><span class=\"since\" title=\"Stable since Rust version 1.0.0\">1.0.0</span> · <a class=\"src\" href=\"https://doc.rust-lang.org/nightly/src/core/clone.rs.html#174\">source</a></span><a href=\"#method.clone_from\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html#method.clone_from\" class=\"fn\">clone_from</a>(&amp;mut self, source: &amp;Self)</h4></section></summary><div class='docblock'>Performs copy-assignment from <code>source</code>. <a href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html#method.clone_from\">Read more</a></div></details></div></details>","Clone","rustix::timespec::Timespec"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-Debug-for-__kernel_timespec\" class=\"impl\"><a class=\"src rightside\" href=\"src/linux_raw_sys/x86_64/general.rs.html#342\">source</a><a href=\"#impl-Debug-for-__kernel_timespec\" class=\"anchor\">§</a><h3 class=\"code-header\">impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/fmt/trait.Debug.html\" title=\"trait core::fmt::Debug\">Debug</a> for <a class=\"struct\" href=\"linux_raw_sys/general/struct.__kernel_timespec.html\" title=\"struct linux_raw_sys::general::__kernel_timespec\">__kernel_timespec</a></h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.fmt\" class=\"method trait-impl\"><a class=\"src rightside\" href=\"src/linux_raw_sys/x86_64/general.rs.html#342\">source</a><a href=\"#method.fmt\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"https://doc.rust-lang.org/nightly/core/fmt/trait.Debug.html#tymethod.fmt\" class=\"fn\">fmt</a>(&amp;self, f: &amp;mut <a class=\"struct\" href=\"https://doc.rust-lang.org/nightly/core/fmt/struct.Formatter.html\" title=\"struct core::fmt::Formatter\">Formatter</a>&lt;'_&gt;) -&gt; <a class=\"enum\" href=\"https://doc.rust-lang.org/nightly/core/result/enum.Result.html\" title=\"enum core::result::Result\">Result</a>&lt;<a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.unit.html\">()</a>, <a class=\"struct\" href=\"https://doc.rust-lang.org/nightly/core/fmt/struct.Error.html\" title=\"struct core::fmt::Error\">Error</a>&gt;</h4></section></summary><div class='docblock'>Formats the value using the given formatter. <a href=\"https://doc.rust-lang.org/nightly/core/fmt/trait.Debug.html#tymethod.fmt\">Read more</a></div></details></div></details>","Debug","rustix::timespec::Timespec"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-PartialEq-for-__kernel_timespec\" class=\"impl\"><a class=\"src rightside\" href=\"src/linux_raw_sys/lib.rs.html#67\">source</a><a href=\"#impl-PartialEq-for-__kernel_timespec\" class=\"anchor\">§</a><h3 class=\"code-header\">impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/cmp/trait.PartialEq.html\" title=\"trait core::cmp::PartialEq\">PartialEq</a> for <a class=\"struct\" href=\"linux_raw_sys/general/struct.__kernel_timespec.html\" title=\"struct linux_raw_sys::general::__kernel_timespec\">__kernel_timespec</a></h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.eq\" class=\"method trait-impl\"><a class=\"src rightside\" href=\"src/linux_raw_sys/lib.rs.html#68\">source</a><a href=\"#method.eq\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"https://doc.rust-lang.org/nightly/core/cmp/trait.PartialEq.html#tymethod.eq\" class=\"fn\">eq</a>(&amp;self, other: &amp;<a class=\"struct\" href=\"linux_raw_sys/general/struct.__kernel_timespec.html\" title=\"struct linux_raw_sys::general::__kernel_timespec\">__kernel_timespec</a>) -&gt; <a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.bool.html\">bool</a></h4></section></summary><div class='docblock'>Tests for <code>self</code> and <code>other</code> values to be equal, and is used by <code>==</code>.</div></details><details class=\"toggle method-toggle\" open><summary><section id=\"method.ne\" class=\"method trait-impl\"><span class=\"rightside\"><span class=\"since\" title=\"Stable since Rust version 1.0.0\">1.0.0</span> · <a class=\"src\" href=\"https://doc.rust-lang.org/nightly/src/core/cmp.rs.html#261\">source</a></span><a href=\"#method.ne\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"https://doc.rust-lang.org/nightly/core/cmp/trait.PartialEq.html#method.ne\" class=\"fn\">ne</a>(&amp;self, other: <a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.reference.html\">&amp;Rhs</a>) -&gt; <a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.bool.html\">bool</a></h4></section></summary><div class='docblock'>Tests for <code>!=</code>. The default implementation is almost always sufficient,\nand should not be overridden without very good reason.</div></details></div></details>","PartialEq","rustix::timespec::Timespec"],["<section id=\"impl-Copy-for-__kernel_timespec\" class=\"impl\"><a class=\"src rightside\" href=\"src/linux_raw_sys/x86_64/general.rs.html#342\">source</a><a href=\"#impl-Copy-for-__kernel_timespec\" class=\"anchor\">§</a><h3 class=\"code-header\">impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/marker/trait.Copy.html\" title=\"trait core::marker::Copy\">Copy</a> for <a class=\"struct\" href=\"linux_raw_sys/general/struct.__kernel_timespec.html\" title=\"struct linux_raw_sys::general::__kernel_timespec\">__kernel_timespec</a></h3></section>","Copy","rustix::timespec::Timespec"],["<section id=\"impl-Eq-for-__kernel_timespec\" class=\"impl\"><a class=\"src rightside\" href=\"src/linux_raw_sys/lib.rs.html#79\">source</a><a href=\"#impl-Eq-for-__kernel_timespec\" class=\"anchor\">§</a><h3 class=\"code-header\">impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/cmp/trait.Eq.html\" title=\"trait core::cmp::Eq\">Eq</a> for <a class=\"struct\" href=\"linux_raw_sys/general/struct.__kernel_timespec.html\" title=\"struct linux_raw_sys::general::__kernel_timespec\">__kernel_timespec</a></h3></section>","Eq","rustix::timespec::Timespec"]]]]);
    if (window.register_type_impls) {
        window.register_type_impls(type_impls);
    } else {
        window.pending_type_impls = type_impls;
    }
})()
//{"start":55,"fragment_lengths":[7822]}