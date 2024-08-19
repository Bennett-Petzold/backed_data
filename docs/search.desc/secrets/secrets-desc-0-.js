searchState.loadedDescShard("secrets", 0, "Protected-access memory for cryptographic secrets.\nA type for protecting secrets allocated on the stack.\nEnsures that the <code>Secret</code>’s underlying memory is <code>munlock</code>ed …\nReturns the argument unchanged.\nCreates a new <code>Secret</code> from existing, unprotected data, and …\nCalls <code>U::from(self)</code>.\nCreates a new <code>Secret</code> and invokes the provided callback with\nCreates a new <code>Secret</code> filled with random bytes and invokes …\nContainer for <code>SecretBox</code>.\nContainer for <code>SecretVec</code>.\nMarker traits to allow types to be contained as secrets.\nCreates a new <code>Secret</code> filled with zeroed bytes and invokes …\nAn immutable wrapper around the internal contents of a …\nA mutable wrapper around the internal contents of a …\nA type for protecting fixed-length secrets allocated on …\nImmutably borrows the contents of the <code>SecretBox</code>. Returns a …\nMutably borrows the contents of the <code>SecretBox</code>. Returns a …\nReturns the argument unchanged.\nReturns the argument unchanged.\nCreates a new <code>SecretBox</code> from existing, unprotected data, …\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nInstantiates and returns a new <code>SecretBox</code>.\nCreates a new <code>SecretBox</code> filled with …\nReturns the size in bytes of the <code>SecretBox</code>.\nInstantiates and returns a new <code>SecretBox</code>. Has equivalent …\nCreates a new <code>SecretBox</code> filled with zeroes.\nA mutable wrapper around an item in a <code>SecretVec</code>.\nAn immutable wrapper around an item in a <code>SecretVec</code>.\nAn iterator for <code>SecretVec</code> that returns <code>ItemMut</code>.\nAn iterator for <code>SecretVec</code> that returns <code>ItemRef</code>.\nAn immutable wrapper around the internal contents of a …\nA mutable wrapper around the internal contents of a …\nA type for protecting variable-length secrets allocated on …\nImmutably borrows the contents of the <code>SecretVec</code>. Returns a …\nMutably borrows the contents of the <code>SecretVec</code>. Returns a …\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nCreates a new <code>SecretVec</code> from existing, unprotected data, …\nReturns the argument unchanged.\nImmutably borrows an item in the <code>SecretVec</code>, returning a …\nMutably borrows an item in the <code>SecretVec</code>, returning a …\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nReturns true if length of the <code>SecretVec</code> is zero.\nWraps <code>into_iter</code>.\nWraps <code>into_iter</code>.\nReturns the number of elements in the <code>SecretVec</code>.\nInstantiates and returns a new <code>SecretVec</code>.\nCreates a new <code>SecretVec</code> with  <code>len</code> elements, filled with …\nReturns the size in bytes of the <code>SecretVec</code>.\nInstantiates and returns a new <code>SecretVec</code>. Has equivalent …\nCreates a new <code>SecretVec</code> with  <code>len</code> elements, filled with …\nMarker trait for types who are intrepretable as a series of\nA marker trait for types whose size is known at compile …\nA marker trait for types that can be compared for equality …\nTypes that can be safely initialized by setting their …\nTypes that can be safely initialized by setting their …\nReturns a byte slice to the underlying data.\nReturns a byte slice to the underlying data.\nReturns a mutable byte slice to the underlying data.\nReturns a mutable byte slice to the underlying data.\nReturns a <code>*mut u8</code> pointer to the beginning of the data.\nReturns a <code>*mut u8</code> pointer to the beginning of the data.\nReturns a <code>*mut u8</code> pointer to the beginning of the data.\nReturns a <code>*const u8</code> pointer to the beginning of the data.\nReturns a <code>*const u8</code> pointer to the beginning of the data.\nReturns a <code>*const u8</code> pointer to the beginning of the data.\nCompares <code>self</code> and <code>rhs</code>. Guaranteed to return false when the …\nCompares <code>self</code> and <code>rhs</code>. Guaranteed to return false when the …\nRandomizes the contents of <code>self</code>.\nRandomizes the contents of <code>self</code>.\nReturns the size in bytes of <code>Self</code>.\nReturns the size in bytes of <code>Self</code>.\nReturns the size in bytes of <code>Self</code>.\nCopies all bytes from <code>self</code> into <code>other</code> before zeroing out …\nCopies all bytes from <code>self</code> into <code>other</code> before zeroing out …\nReturns an uninitialized value.\nReturns an uninitialized value.\nZeroes out the underlying storage.\nZeroes out the underlying storage.")