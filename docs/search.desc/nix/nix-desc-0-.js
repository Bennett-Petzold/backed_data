searchState.loadedDescShard("nix", 0, "Rust friendly bindings to the various *nix system …\nContains the error value\nNix’s main error type.\nCommon trait used to represent file system paths by many …\nContains the success value\nNix Result Type\nSafe wrappers around errno functions\nfile control options\nIs the path empty?\nLength of the path in bytes\nMostly platform-specific functionality\nSafe wrappers around functions found in libc “unistd.h”…\nExecute a function with this path as a <code>CStr</code>.\nThe sentinel value indicates that a function failed and …\nSets the platform-specific errno to no-error\nReturns the platform-specific value of errno\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nReturns the current value of errno\nReturns the current raw i32 value of errno\nReturns <code>Ok(value)</code> if it does not contain the sentinel …\nSets the value of errno.\nSets the raw i32 value of errno.\nUsed with <code>faccessat</code>, the checks for accessibility are …\nIf the provided path is an empty string, operate on the …\nDon’t automount the terminal (“basename”) component …\nUsed with <code>linkat</code> to create a link to a symbolic link’s …\nUsed with functions like <code>fstatat</code> to operate on a link …\nFlags that control how the various *at syscalls behave.\nRemoves byte range from a file without leaving a hole.\nIncreases file space by inserting a hole within the file …\nFile size is not changed.\nDeallocates space by creating a hole.\nShared file data extants are made private to the file.\nZeroes space in specified byte range.\nThe file descriptor will automatically be closed during a …\nAdd seals to the file\nDuplicate the provided file descriptor\nDuplicate the provided file descriptor and set the …\nGet the close-on-exec flag associated with the file …\nGet descriptor status flags\nGet the first lock that blocks the lock description\nReturn the capacity of a pipe\nGet seals associated with the file\nDetermine whether it would be possible to create the given …\nAcquire or release an open file description lock\nLike <code>F_OFD_SETLK</code> except that if a conflicting lock is held …\nThe file contents cannot be modified, except via shared …\nThe size of the file cannot be increased.\nPrevents further calls to <code>fcntl()</code> with <code>F_ADD_SEALS</code>.\nThe file cannot be reduced in size.\nThe file contents cannot be modified.\nSet the close-on-exec flag associated with the file …\nSet descriptor status flags\nSet or clear a file segment lock\nLike <code>F_SETLK</code> except that if a shared or exclusive lock is …\nChange the capacity of a pipe\nMode argument flags for fallocate determining operation …\nCommands for use with <code>fcntl</code>.\nAdditional configuration flags for <code>fcntl</code>’s <code>F_SETFD</code>.\nRepresents an owned flock, which unlocks on drop.\nOperations for use with <code>Flock::lock</code>.\nRepresents valid types for flock.\nexclusive file lock\nExclusive lock.  Do not block when locking.\nshared file lock\nShared lock.  Do not block when locking.\nConfiguration options for opened files.\nMask for the access mode of the file.\nOpen the file in append-only mode.\nGenerate a signal when input or output becomes possible.\nCloses the file descriptor once an <code>execve</code> call is made.\nCreate the file if it does not exist.\nTry to minimize cache effects of the I/O for this file.\nIf the specified path isn’t a directory, fail.\nImplicitly follow each <code>write()</code> with an <code>fdatasync()</code>.\nError out if a file was not created.\nSame as <code>O_SYNC</code>.\nAllow files whose sizes can’t be represented in an <code>off_t</code> …\nSame as <code>O_NONBLOCK</code>.\nDo not update the file last access time during <code>read(2)</code>s.\nDon’t attach the device as the process’ controlling …\n<code>open()</code> will fail if the given path is a symbolic link.\nWhen possible, open the file in nonblocking mode.\nObtain a file descriptor for low-level access.\nOnly allow reading.\nAllow both reading and writing.\nSimilar to <code>O_DSYNC</code> but applies to <code>read</code>s instead.\nImplicitly follow each <code>write()</code> with an <code>fsync()</code>.\nCreate an unnamed temporary file.\nTruncate an existing regular file to 0 length if it allows …\nOnly allow writing.\nSpecifies how openat2 should open a pathname.\nThe specified data will not be accessed in the near future.\nThe specified data will only be accessed once and then not …\nRevert to the default data access behavior.\nA hint that file data will be accessed randomly, and …\nThe file data will be accessed sequentially.\nThe specified data will be accessed in the near future.\nThe specific advice provided to <code>posix_fadvise</code>.\nAtomically exchange <code>old_path</code> and <code>new_path</code>.\nDon’t overwrite <code>new_path</code> of the rename.  Return an error …\ncreates a “whiteout” object at the source of the …\nDo not permit the path resolution to succeed if any …\nTreat the directory referred to by dirfd as the root …\nDisallow all magic-link resolution during path resolution. …\nDisallow resolution of symbolic links during path …\nDisallow traversal of mount points during path resolution …\nFlags for use with <code>renameat2</code>.\nPath resolution flags.\nAdditional flags for file sealing, which allows for …\nUnlock file\nGet a flags value with all known bits set.\nGet a flags value with all known bits set.\nGet a flags value with all known bits set.\nGet a flags value with all known bits set.\nGet a flags value with all known bits set.\nGet a flags value with all known bits set.\nGet a flags value with all known bits set.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nGet the underlying bits value.\nGet the underlying bits value.\nGet the underlying bits value.\nGet the underlying bits value.\nGet the underlying bits value.\nGet the underlying bits value.\nGet the underlying bits value.\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nWhether all set bits in a source flags value are also set …\nWhether all set bits in a source flags value are also set …\nWhether all set bits in a source flags value are also set …\nWhether all set bits in a source flags value are also set …\nWhether all set bits in a source flags value are also set …\nWhether all set bits in a source flags value are also set …\nWhether all set bits in a source flags value are also set …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nGet a flags value with all bits unset.\nGet a flags value with all bits unset.\nGet a flags value with all bits unset.\nGet a flags value with all bits unset.\nGet a flags value with all bits unset.\nGet a flags value with all bits unset.\nGet a flags value with all bits unset.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nManipulates file space.\nPerform various operations on open file descriptors.\nSet the open flags used to open a file, completely …\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nConvert from a bits value.\nConvert from a bits value.\nConvert from a bits value.\nConvert from a bits value.\nConvert from a bits value.\nConvert from a bits value.\nConvert from a bits value.\nConvert from a bits value exactly.\nConvert from a bits value exactly.\nConvert from a bits value exactly.\nConvert from a bits value exactly.\nConvert from a bits value exactly.\nConvert from a bits value exactly.\nConvert from a bits value exactly.\nConvert from a bits value, unsetting any unknown bits.\nConvert from a bits value, unsetting any unknown bits.\nConvert from a bits value, unsetting any unknown bits.\nConvert from a bits value, unsetting any unknown bits.\nConvert from a bits value, unsetting any unknown bits.\nConvert from a bits value, unsetting any unknown bits.\nConvert from a bits value, unsetting any unknown bits.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nGet a flags value with the bits of a flag with the given …\nGet a flags value with the bits of a flag with the given …\nGet a flags value with the bits of a flag with the given …\nGet a flags value with the bits of a flag with the given …\nGet a flags value with the bits of a flag with the given …\nGet a flags value with the bits of a flag with the given …\nGet a flags value with the bits of a flag with the given …\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nWhether any set bits in a source flags value are also set …\nWhether any set bits in a source flags value are also set …\nWhether any set bits in a source flags value are also set …\nWhether any set bits in a source flags value are also set …\nWhether any set bits in a source flags value are also set …\nWhether any set bits in a source flags value are also set …\nWhether any set bits in a source flags value are also set …\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nWhether all known bits in this flags value are set.\nWhether all known bits in this flags value are set.\nWhether all known bits in this flags value are set.\nWhether all known bits in this flags value are set.\nWhether all known bits in this flags value are set.\nWhether all known bits in this flags value are set.\nWhether all known bits in this flags value are set.\nWhether all bits in this flags value are unset.\nWhether all bits in this flags value are unset.\nWhether all bits in this flags value are unset.\nWhether all bits in this flags value are unset.\nWhether all bits in this flags value are unset.\nWhether all bits in this flags value are unset.\nWhether all bits in this flags value are unset.\nYield a set of contained flags values.\nYield a set of contained flags values.\nYield a set of contained flags values.\nYield a set of contained flags values.\nYield a set of contained flags values.\nYield a set of contained flags values.\nYield a set of contained flags values.\nYield a set of contained named flags values.\nYield a set of contained named flags values.\nYield a set of contained named flags values.\nYield a set of contained named flags values.\nYield a set of contained named flags values.\nYield a set of contained named flags values.\nYield a set of contained named flags values.\nObtain a/an flock.\nSet the file mode new files will be created with, …\nCreate a new zero-filled <code>open_how</code>.\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nopen or create a file for reading, writing or executing\nopen or create a file for reading, writing or executing\nOpen or create a file for reading, writing or executing.\nAllows a process to describe to the system its data access …\nPre-allocate storage for a range in a file\nRead value of a symbolic link\nRead value of a symbolic link.\nRelock the file.  This can upgrade or downgrade the lock …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nChange the name of a file.\nLike <code>renameat</code>, but with an additional <code>flags</code> argument.\nSet resolve flags, completely overwriting any existing …\nCall <code>insert</code> when <code>value</code> is <code>true</code> or <code>remove</code> when <code>value</code> is …\nCall <code>insert</code> when <code>value</code> is <code>true</code> or <code>remove</code> when <code>value</code> is …\nCall <code>insert</code> when <code>value</code> is <code>true</code> or <code>remove</code> when <code>value</code> is …\nCall <code>insert</code> when <code>value</code> is <code>true</code> or <code>remove</code> when <code>value</code> is …\nCall <code>insert</code> when <code>value</code> is <code>true</code> or <code>remove</code> when <code>value</code> is …\nCall <code>insert</code> when <code>value</code> is <code>true</code> or <code>remove</code> when <code>value</code> is …\nCall <code>insert</code> when <code>value</code> is <code>true</code> or <code>remove</code> when <code>value</code> is …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nRemove the lock and return the object wrapped within.\nInterfaces for managing memory-backed files.\nOperating system signals.\nGet filesystem statistics, non-portably\nGet filesystem statistics\nAllow sealing operations on this file.\nSet the close-on-exec (<code>FD_CLOEXEC</code>) flag on the new file …\nAnonymous file will be created using huge pages. It should …\nhugetlb size of 16GB.\nhugetlb size of 16MB.\nhugetlb size of 1GB.\nFollowing are to be used with [<code>MFD_HUGETLB</code>], indicating …\nhugetlb size of 256MB.\nhugetlb size of 2GB.\nhugetlb size of 2MB.\nhugetlb size of 32MB.\nhugetlb size of 512MB.\nhugetlb size of 8MB.\nOptions that change the behavior of <code>memfd_create</code>.\nGet a flags value with all known bits set.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nGet the underlying bits value.\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nWhether all set bits in a source flags value are also set …\nThe intersection of a source flags value with the …\nGet a flags value with all bits unset.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nReturns the argument unchanged.\nConvert from a bits value.\nConvert from a bits value exactly.\nConvert from a bits value, unsetting any unknown bits.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nGet a flags value with the bits of a flag with the given …\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nWhether any set bits in a source flags value are also set …\nCalls <code>U::from(self)</code>.\nWhether all known bits in this flags value are set.\nWhether all bits in this flags value are unset.\nYield a set of contained flags values.\nYield a set of contained named flags values.\nCreates an anonymous file that lives in memory, and return …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nThe intersection of a source flags value with the …\nCall <code>insert</code> when <code>value</code> is <code>true</code> or <code>remove</code> when <code>value</code> is …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nFlags for <code>fchmodat</code> function.\n“File mode / permissions” flags.\n“File type” flags for <code>mknod</code> and related functions.\nRead for group.\nRead for other.\nRead for owner.\nRead write and execute for group.\nRead, write and execute for other.\nRead, write and execute for owner.\nSet group id on execution.\nSet user id on execution.\nWrite for group.\nWrite for other.\nWrite for owner.\nExecute for group.\nExecute for other.\nExecute for owner.\nFlags for <code>utimensat</code> function.\nGet a flags value with all known bits set.\nGet a flags value with all known bits set.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nGet the underlying bits value.\nGet the underlying bits value.\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nWhether all set bits in a source flags value are also set …\nWhether all set bits in a source flags value are also set …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nGet a flags value with all bits unset.\nGet a flags value with all bits unset.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nChange the file permission bits of the file specified by a …\nChange the file permission bits.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nConvert from a bits value.\nConvert from a bits value.\nConvert from a bits value exactly.\nConvert from a bits value exactly.\nConvert from a bits value, unsetting any unknown bits.\nConvert from a bits value, unsetting any unknown bits.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nGet a flags value with the bits of a flag with the given …\nGet a flags value with the bits of a flag with the given …\nChange the access and modification times of the file …\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nWhether any set bits in a source flags value are also set …\nWhether any set bits in a source flags value are also set …\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nWhether all known bits in this flags value are set.\nWhether all known bits in this flags value are set.\nWhether all bits in this flags value are unset.\nWhether all bits in this flags value are unset.\nYield a set of contained flags values.\nYield a set of contained flags values.\nYield a set of contained named flags values.\nYield a set of contained named flags values.\nChange the access and modification times of a file without …\nCreate a special or ordinary file, by pathname.\nCreate a special or ordinary file, relative to a given …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nCall <code>insert</code> when <code>value</code> is <code>true</code> or <code>remove</code> when <code>value</code> is …\nCall <code>insert</code> when <code>value</code> is <code>true</code> or <code>remove</code> when <code>value</code> is …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nChange the access and modification times of a file.\nChange the access and modification times of a file.\nDescribes the file system type as known by the operating …\nDescribes a mounted file system\nSize of a block\nTotal data blocks in filesystem\nFree blocks available to unprivileged user\nFree blocks in filesystem\nTotal file nodes in filesystem\nFree file nodes in filesystem\nFilesystem ID\nMagic code defining system type\nGet the mount flags\nReturns the argument unchanged.\nReturns the argument unchanged.\nIdentifies a mounted file system\nDescribes a mounted file system.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nMaximum length of filenames\nOptimal transfer block size\nDescribes a mounted file system.\nFile system mount Flags\nAppend-only file\nImmutable file\nAllow mandatory locks on the filesystem\nDo not update access times on files\nDo not interpret character or block-special devices\nDo not update access times on files\nDo not allow execution of binaries on the filesystem\nDo not allow the set-uid bits to have an effect\nRead Only\nUpdate access time relative to modify/change time\nAll IO should be done synchronously\nWrite on file/directory/symlink\nWrapper around the POSIX <code>statvfs</code> struct\nGet a flags value with all known bits set.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nGet the underlying bits value.\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nget the file system block size\nGet the number of blocks.\nGet the number of free blocks for unprivileged users\nGet the number of free blocks in the file system\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nWhether all set bits in a source flags value are also set …\nThe intersection of a source flags value with the …\nGet a flags value with all bits unset.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nGet the total number of file inodes\nGet the number of free file inodes for unprivileged users\nGet the number of free file inodes\nGet the file system id\nGet the mount flags\nGet the fundamental file system block size\nReturns the argument unchanged.\nReturns the argument unchanged.\nConvert from a bits value.\nConvert from a bits value exactly.\nConvert from a bits value, unsetting any unknown bits.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nGet a flags value with the bits of a flag with the given …\nReturn a <code>Statvfs</code> object with information about <code>fd</code>\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nWhether any set bits in a source flags value are also set …\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nWhether all known bits in this flags value are set.\nWhether all bits in this flags value are unset.\nYield a set of contained flags values.\nYield a set of contained named flags values.\nGet the maximum filename length\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nThe intersection of a source flags value with the …\nCall <code>insert</code> when <code>value</code> is <code>true</code> or <code>remove</code> when <code>value</code> is …\nReturn a <code>Statvfs</code> object with information about the <code>path</code>\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nSystem info structure returned by <code>sysinfo</code>.\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nReturns the load average tuple.\nCurrent number of processes.\nReturns the total amount of installed RAM in Bytes.\nReturns the amount of completely unused RAM in Bytes.\nReturns the amount of unused swap memory in Bytes.\nReturns the amount of swap memory in Bytes.\nReturns system information.\nReturns the time since system boot.\nUpdate the timestamp to <code>Now</code>\nLeave the timestamp unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nMakes a new <code>TimeSpec</code> with given number of microseconds.\nMakes a new <code>TimeVal</code> with given number of microseconds.\nMakes a new <code>TimeSpec</code> with given number of nanoseconds.\nMakes a new <code>TimeVal</code> with given number of nanoseconds.  …\nConstruct a new <code>TimeSpec</code> from its components\nConstruct a new <code>TimeVal</code> from its components\nOptions for access()\nTest for existence of file.\nRemove the directory entry as a normal file, not a …\nTest for read permission.\nRemove the directory entry as a directory, not a normal …\nSpecify an offset relative to the current file location.\nSpecify an offset relative to the next location in the …\nSpecify an offset relative to the end of the file.\nSpecify an offset relative to the next hole in the file …\nSpecify an offset relative to the start of the file.\nFlags for <code>unlinkat</code> function.\nTest for write permission.\nDirective that tells <code>lseek</code> and <code>lseek64</code> what the offset is …\nTest for execute (search) permission.\nChecks the file named by <code>path</code> for accessibility according …\nGet a flags value with all known bits set.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nGet the underlying bits value.\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nChange the current working directory of the calling …\nChange a process’s root directory\nClose a raw file descriptor\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nWhether all set bits in a source flags value are also set …\nThe intersection of a source flags value with the …\nCreate a copy of the specified file descriptor (see dup(2)…\nCreate a copy of the specified file descriptor using the …\nCreate a new copy of the specified file descriptor using …\nChecks the file named by <code>path</code> for accessibility according …\nGet a flags value with all bits unset.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nChecks the file named by <code>path</code> for accessibility according …\nChange the current working directory of the process to the …\nSynchronize the data of a file\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nConvert from a bits value.\nConvert from a bits value exactly.\nConvert from a bits value, unsetting any unknown bits.\nThe bitwise or (<code>|</code>) of the bits in each flags value.\nGet a flags value with the bits of a flag with the given …\nSynchronize changes to a file\nTruncate a file to a specified length\nReturns the current directory as a <code>PathBuf</code>\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nThe bitwise and (<code>&amp;</code>) of the bits in two flags values.\nWhether any set bits in a source flags value are also set …\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nWhether all known bits in this flags value are set.\nWhether all bits in this flags value are unset.\nDetermines if the file descriptor refers to a valid …\nYield a set of contained flags values.\nYield a set of contained named flags values.\nLink one file to another file\nMove the read/write file offset.\nMove the read/write file offset.\nCreates new directory <code>path</code> with access rights <code>mode</code>.  (see …\nCreates new fifo special file (named pipe) with path <code>path</code> …\nCreates new fifo special file (named pipe) with path <code>path</code> …\nCreates a regular file which persists even after process …\nThe bitwise negation (<code>!</code>) of the bits in a flags value, …\nCreate an interprocess channel.\nLike <code>pipe</code>, but allows setting certain file descriptor …\nChange the root file system.\nRead from a raw file descriptor.\nThe intersection of a source flags value with the …\nCall <code>insert</code> when <code>value</code> is <code>true</code> or <code>remove</code> when <code>value</code> is …\nSuspend execution for an interval of time\nThe intersection of a source flags value with the …\nThe intersection of a source flags value with the …\nCreates a symbolic link at <code>path2</code> which points to <code>path1</code>.\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nCommit filesystem caches to disk\nCommit filesystem caches containing file referred to by …\nThe bitwise exclusive-or (<code>^</code>) of the bits in two flags …\nTruncate a file to a specified length\nThe bitwise or (<code>|</code>) of the bits in two flags values.\nRemove a directory entry\nRemove a directory entry\nWrite to a raw file descriptor.")