#[macro_use]
extern crate proptest;
extern crate nbd;
extern crate pipe;
extern crate readwrite;

use proptest::prelude::{prop, Just, ProptestConfig, Strategy};
use proptest::string::bytes_regex;

use std::io::{Cursor, Read, Write};

use readwrite::ReadWrite;

fn gen_chunk() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof! {
        Just(b"IHAVEOPT".to_vec()),
        Just(b"NBDMAGIC".to_vec()),
        Just(b"\x00\x00".to_vec()),
        bytes_regex("\x00\x00\x00\x00\x00\x00..").unwrap(),
        bytes_regex("\x00\x00\x00.").unwrap(),
        bytes_regex("\x00.").unwrap(),
        bytes_regex("\x00[\x00-\x0F]").unwrap(),
        bytes_regex(".").unwrap(),
        Just(vec![0; 124]),
        Just(b"\x00\x00\x00\x01".to_vec()),
        Just(b"\x00\x00\x42\x02\x81\x86\x12\x53".to_vec()),
        Just(b"\x00\x03\xe8\x89\x04\x55\x65\xa9".to_vec()),
        Just(b"\x49\x48\x41\x56\x45\x4F\x50\x54".to_vec()),
    }
}

fn get_random_socket(chunks: Vec<Vec<u8>>) -> impl Read + Write {
    let input: Vec<u8> = chunks.iter().flatten().map(|x| *x).collect();
    let socket = ReadWrite::new(Cursor::new(input), ::std::io::sink());
    socket
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 10_000,
        .. ProptestConfig::default()
    })]

    #[test]
    fn fuzz_client_hs(chunks in prop::collection::vec(gen_chunk(),3..12)) {
        let s = get_random_socket(chunks);
        let ret = nbd::client::handshake(s, b"");
        if let Ok(x) = ret {
            eprintln!("Happy case {:?}", x);
        } else {
            // Error, but not panic is fine too
        }
    }


    #[test]
    fn fuzz_server_hs(chunks in prop::collection::vec(gen_chunk(),3..12)) {
        let s = get_random_socket(chunks);
        let ret = nbd::server::handshake(s, |_| {
            Ok(nbd::server::Export::<()>::default())
        });
        if let Ok(x) = ret {
            eprintln!("Happy case {:?}", x);
        } else {
            // Error, but not panic is fine too
        }
    }
}
