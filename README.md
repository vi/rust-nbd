rust-nbd
---

[Network block device](https://en.wikipedia.org/wiki/Network_block_device) protocol implementation in Rust. For now, only server, only one export and not all features.

Accepts a `Read`+`Write`+`Seek` as a data to be exposed.

This library is IO-agnostic, but async is not supported.

See [the example](https://github.com/vi/rust-nbd/blob/master/examples/server.rs).

This is an early version.
