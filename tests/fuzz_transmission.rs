#[macro_use]
extern crate proptest;
extern crate nbd;
extern crate pipe;
extern crate readwrite;

use proptest::prelude::{prop, Just, ProptestConfig, Strategy};
use proptest::string::bytes_regex;

use std::io::{Cursor, Read, Result, Seek, SeekFrom, Write};

use readwrite::ReadWrite;

#[derive(Debug, Eq, PartialEq)]
enum Action {
    Seek(u64),
    Write(usize),
    Read(usize),
}

const SS: u64 = 1024 * 1024;

prop_compose! {
    fn biased_size()(x in 0..32usize, y in 0..3u8) -> usize {
        if y == 0 {
            x
        } else {
            x % 6
        }
    }
}

fn gen_action() -> impl Strategy<Value = Action> {
    prop_oneof! {
        (0..SS).prop_map(Action::Seek),
        biased_size().prop_map(Action::Write),
        biased_size().prop_map(Action::Read),
    }
}

fn gen_chunk() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof! {
        Just(b"\x25\x60\x95\x13".to_vec()),
        Just(b"\x67\x44\x66\x98".to_vec()),
        Just(b"\x00\x00\x00".to_vec()),
        Just(b"\x00\x00\x00\x00\x00\x00\x00[\x00-\x1F]".to_vec()),
        Just(b"\x00\x00".to_vec()),
        Just(b"\xFF\xFF".to_vec()),
        bytes_regex(".").unwrap(),
        bytes_regex("\x00[\x01-\x09]").unwrap(),
    }
}

fn get_random_socket(chunks: Vec<Vec<u8>>) -> impl Read + Write {
    let input: Vec<u8> = chunks.iter().flatten().map(|x| *x).collect();
    let socket = ReadWrite::new(Cursor::new(input), ::std::io::sink());
    socket
}

struct FakeStorage;

impl Read for FakeStorage {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        Ok(buf.len())
    }
}
impl Write for FakeStorage {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        Ok(buf.len())
    }
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}
impl Seek for FakeStorage {
    fn seek(&mut self, _: SeekFrom) -> Result<u64> {
        Ok(1024_000)
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 20_000,
        .. ProptestConfig::default()
    })]

    #[test]
    fn fuzz_client_transmission(chunks in prop::collection::vec(gen_chunk(),3..30),
                                script in prop::collection::vec(gen_action(),3..12),
        ) {
        let s = get_random_socket(chunks);
        let mut exp = nbd::client::Export::default();
        exp.size = SS;
        let mut client = nbd::client::NbdClient::new(s, &exp);

        let mut buf = vec![0;4096];

        let mut succ = 0;

        for i in script {
            match i {
                Action::Seek(pos) => {
                    client.seek(SeekFrom::Start(pos)).unwrap();
                },
                Action::Write(sz) => {
                    if sz > 0 {
                        if client.write(&buf[0..sz]).is_ok() { succ += 1; }
                    }
                },
                Action::Read(sz) => {
                    if sz > 0 {
                        if client.read(&mut buf[0..sz]).is_ok() { succ += 1; }
                    }
                },
            }
        }

        if succ > 1 {
            eprintln!("succ={}", succ);
        }
    }


    #[test]
    fn fuzz_server_transmission(chunks in prop::collection::vec(gen_chunk(),3..12)) {
        let s = get_random_socket(chunks);
        let ret = nbd::server::transmission(s, FakeStorage);
        if ret.is_ok() {
            eprintln!("Happy");
        }
    }
}
