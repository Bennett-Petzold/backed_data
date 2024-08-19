searchState.loadedDescShard("zstd_safe", 0, "Minimal safe wrapper around zstd-sys.\nCompression context\nCompression dictionary.\nA compression parameter.\nCompression stream.\nRepresents the compression level used by zstd.\nCompression level to use.\nIndicates an error happened when parsing the frame content …\nA Decompression Context.\nA digested decompression dictionary.\nA decompression parameter.\nA Decompression stream.\nContains the error value\nRepresents a possible error from the zstd library.\nWrapper around an input buffer.\nSize in bytes of a compression job.\nHow many threads will be spawned.\nContains the success value\nWrapper around an output buffer.\nSpecifies how much overlap must be given to each worker.\nOnly reset parameters (including dictionary or referenced …\nWhat kind of context reset should be applied.\nWrapper result around most zstd functions.\nReset both the session and parameters.\nOnly the session will be reset.\nHow to compress data. Advanced compression API (Requires …\nTarget CBlock size.\nMaximum allowed back-reference distance.\nDescribe a bytes container, like <code>Vec&lt;u8&gt;</code>.\nReturns a new <code>InBuffer</code> around the given slice.\nReturns a new <code>OutBuffer</code> around the given slice.\nReturns a new <code>OutBuffer</code> around the given slice, starting …\nReturns a pointer to the start of the data.\nReturns a pointer to the start of this buffer.\nReturns the valid data part of this container. Should only …\nReturns the part of this buffer that was written to.\nReturns the full capacity of this container. May include …\nReturns the capacity of the underlying buffer.\nWraps the <code>ZSTD_compress</code> function.\nWraps the <code>ZSTD_compressCCtx()</code> function\nWraps the <code>ZSTD_compress2()</code> function.\nMaximum compressed size in worst case single-pass scenario\nPerforms a step of a streaming compression operation.\nPerforms a step of a streaming compression operation.\nWraps the <code>ZSTD_compress_usingCDict()</code> function.\nWraps the <code>ZSTD_compress_usingCDict()</code> function.\nWraps the <code>ZSTD_compress_usingDict()</code> function.\nWrap <code>ZSTD_createCCtx</code>\nCreates a new decoding context.\nPrepare a dictionary to compress data.\nWraps the <code>ZSTD_createCDict()</code> function.\nAllocates a new <code>CStream</code>.\nWraps the <code>ZSTD_createDDict()</code> function.\nWraps the <code>ZSTD_decompress</code> function.\nFully decompress the given frame.\nPerforms a step of a streaming decompression operation.\nWraps the <code>ZSTD_decompress_usingDDict()</code> function.\nFully decompress the given frame using a dictionary.\nFully decompress the given frame using a dictionary.\nReturn to “no-dictionary” mode.\nReturn to “no-dictionary” mode.\nEnds the stream.\nIndicates that the first <code>n</code> bytes of the container have …\nWraps the <code>ZSTD_findFrameCompressedSize()</code> function.\nFlush any intermediate buffer.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nWraps the <code>ZSTD_getDecompressedSize</code> function.\nWraps the <code>ZDICT_getDictID()</code> function.\nReturns the dictionary ID for this dict.\nReturns the dictionary ID for this dict.\nWraps the <code>ZSTD_getDictID_fromDict()</code> function.\nWraps the <code>ZSTD_getDictID_fromFrame()</code> function.\nReturns the error string associated with an error code.\nWraps the <code>ZSTD_getFrameContentSize()</code> function.\nReturns the recommended input buffer size.\nWraps the <code>ZSTD_DStreamInSize()</code> function.\nInitializes the context with the given compression level.\nInitializes an existing <code>DStream</code> for decompression.\nPrepares an existing <code>CStream</code> for compression at the given …\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nTries to load a dictionary.\nLoads a dictionary.\nReturns the maximum (slowest) compression level supported.\nReturns the minimum (fastest) compression level supported.\nReturns the recommended output buffer size.\nWraps the <code>ZSTD_DStreamOutSize()</code> function.\nReturns the current cursor position.\nReturns the current cursor position.\nWraps the <code>ZSTD_CCtx_refCDict()</code> function.\nReferences a dictionary.\nUse some prefix as single-use dictionary for the next …\nUse some prefix as single-use dictionary for the next …\nResets the state of the context.\nResets the state of the context.\nSets a compression parameter.\nSets a decompression parameter.\nGuarantee that the input size will be this value.\nSets the new cursor position.\nSets the new cursor position.\nReturns the size currently used by this context.\nWraps the <code>ZSTD_sizeof_DCtx()</code> function.\nReturns the <em>current</em> memory usage of this dictionary.\nWraps the <code>ZDICT_trainFromBuffer()</code> function.\nTries to create a new context.\nTry to create a new decompression context.\nPrepare a dictionary to compress data.\nReturns the ZSTD version.\nReturns a string representation of the ZSTD version.\nCall the given closure using the pointer and capacity from …")