# junction

Library for working with NTFS junctions.

[![Build Status][azure-badge]][azure-url]
[![Documentation](https://docs.rs/junction/badge.svg)](https://docs.rs/junction)
[![Crates.io](https://img.shields.io/crates/v/junction.svg)](https://crates.io/crates/junction)

## Usage

Tested in Rust version: Lastest stable

Add this to your `Cargo.toml`:

```toml
[dependencies]
junction = "0.1"
```

[azure-badge]: https://dev.azure.com/taolzu/junction/_apis/build/status/lzutao.junction?branchName=master
[azure-url]: https://dev.azure.com/taolzu/junction/_build/latest?definitionId=3&branchName=master

## Documentations when developing this crate

* https://www.codeproject.com/Articles/194/Windows-2000-Junction-Points#The_Solution
* https://www.codeproject.com/Articles/15633/Manipulating-NTFS-Junction-Points-in-NET
* http://www.flexhex.com/docs/articles/hard-links.phtml
* https://googleprojectzero.blogspot.com/2016/02/the-definitive-guide-on-win32-to-nt.html
* https://github.com/googleprojectzero/symboliclink-testing-tools/blob/master/DumpReparsePoint/DumpReparsePoint.cpp
* https://github.com/googleprojectzero/symboliclink-testing-tools/blob/master/CommonUtils/ReparsePoint.cpp
* https://github.com/containerd/continuity/blob/master/syscallx/syscall_windows.go

## License

All the code in this repository is released under the MIT License,
for more information take a look at the COPYRIGHT file.
