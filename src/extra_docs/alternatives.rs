/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/*!
`backed_data` provides a way to store large data, but it is not the only way.
Each section addresses an alternative approach with some broad pros and cons.

# Direct parsing of source
If the access is perfectly sequential and once per element, there's rarely any reason to not just parse it directly
and drop each line when it is no longer used.
If it's an expensive parse, it still may make more sense to extract the relevant elements one time and parse that sequentially.

# SQL database
This ranges from PostgreSQL (complex remote server) to SQLite (local, embedded-focused library).

Advantages:
* Complex queries with SQL language
* ACID transactions
* Scaling across multiple devices
* Multiple clients can read and modify the data in parallel

Disadvantages:
* Overhead to maintain ACID
* Extra processing cost involved in interpreting SQL

# NoSQL database
This includes many databases such as MongoDB and and Redis.

Advantages:
* Eventual consistency
* Scaling across multiple devices
* Multiple clients can read and modify the data in parallel

Disadvantages:
* Overhead for various database capabilities

# Direct memory mapping
Memory mapping is available on all major operating systems. It models a file
as an array in memory directly, using page faults to load in data when read.

Advantages:
* Silently loads data when needed, and unloads data when memory is low.
* Minimal resource cost, as it uses simple OS calls and structures.

Disadvantages:
* Direct memory representation of data in Rust is not stable, so data is not guaranteed to
    be retrievable after programs are recompiled.
    * To ensure correct representation, the entire dataset needs to be rewritten into the memory
        mapped resource on every execution.
* Other processes can cause memory unsafety by overwriting backing files.
* Multiple threads cannot update the program's memory space at once, so all loading
    from disk is serial.

# Swap (use no solution)
Swap will use disk space to store excess memory.
It does not reuse an allocation on disk -- if you naively read in 16 GB of data from disk on a system with 8 GB RAM,
24 GB (16 + 8) of your disk space is being used to hold that data.
All the other approaches allow for minimizing this duplication to disk.
*/
