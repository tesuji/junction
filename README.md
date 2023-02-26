# junction

Library for working with NTFS junctions.

[![Build Status][actions-badge]][actions-url]
[![Documentation](https://docs.rs/junction/badge.svg)](https://docs.rs/junction)
[![Crates.io](https://img.shields.io/crates/v/junction.svg)](https://crates.io/crates/junction)

### Minimal Supported Rust versions

1.48.0

## All relevant references

* https://www.codeproject.com/Articles/194/Windows-2000-Junction-Points#The_Solution
* https://www.codeproject.com/Articles/15633/Manipulating-NTFS-Junction-Points-in-NET
* http://www.flexhex.com/docs/articles/hard-links.phtml
* https://googleprojectzero.blogspot.com/2016/02/the-definitive-guide-on-win32-to-nt.html
* https://github.com/googleprojectzero/symboliclink-testing-tools/blob/master/DumpReparsePoint/DumpReparsePoint.cpp
* https://github.com/googleprojectzero/symboliclink-testing-tools/blob/master/CommonUtils/ReparsePoint.cpp
* https://github.com/containerd/continuity/blob/master/syscallx/syscall_windows.go

## License

All the code in this repository is released under the MIT License,
for more information, please read COPYRIGHT file.

[actions-badge]: https://github.com/lzutao/junction/workflows/Rust/badge.svg?branchName=master
[actions-url]: https://github.com/lzutao/junction/actions
