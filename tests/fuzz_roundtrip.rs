#[macro_use]
extern crate proptest;
extern crate nbd;
extern crate pipe;
extern crate rand;
extern crate readwrite;

use rand::prng::XorShiftRng;
use rand::{RngCore, SeedableRng};

use proptest::prelude::{prop, Strategy, ProptestConfig};

use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use readwrite::ReadWrite;

#[derive(Debug, Eq, PartialEq)]
enum Action {
    Seek(u64),
    Write(usize),
    ReadAndCheck(usize),
}

const SS: u64 = 1024 * 1024;

prop_compose! {
    fn biased_size()(x in 0..65536usize, y in 0..3u8) -> usize {
        if y == 0 {
            x
        } else {
            x % 2048
        }
    }
}

fn gen_action() -> impl Strategy<Value = Action> {
    prop_oneof! {
        (0..SS).prop_map(Action::Seek),
        biased_size().prop_map(Action::Write),
        biased_size().prop_map(Action::ReadAndCheck),
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        .. ProptestConfig::default()
    })]
    
    #[test]
    fn fuzz_roundtrip(script in prop::collection::vec(gen_action(),3..12)) {
        let seed = [4u8;16];
        let mut r = XorShiftRng::from_seed(seed);

        let mut buf = vec![0;65536];
        let mut buf2 = vec![0;65536];

        let backing_storage1 = vec![0u8;SS as usize];
        let backing_storage2 = vec![0u8;SS as usize];

        let mut c1 = Cursor::new(backing_storage1);
        let mut c2 = Cursor::new(backing_storage2);

        let (r1,w1) = pipe::pipe();
        let (r2,w2) = pipe::pipe();
        let (s1,s2) = (ReadWrite::new(r1,w2), ReadWrite::new(r2,w1));

        let h = std::thread::spawn(move || {
            let _ = nbd::server::transmission(s2, &mut c2);
            c2
        });

        let mut c2 = nbd::client::NbdClient::new(s1, &nbd::Export{size : SS,..Default::default()});

        for i in script {
            match i {
                Action::Seek(pos) => {
                    c1.seek(SeekFrom::Start(pos)).unwrap();
                    c2.seek(SeekFrom::Start(pos)).unwrap();
                },
                Action::Write(mut sz) => {
                    if sz > (SS - c1.position()) as usize {
                        sz = (SS - c1.position()) as usize;
                    }
                    let bufview = &mut buf[0..sz];
                    r.fill_bytes(bufview);
                    c1.write_all(bufview).unwrap();
                    c2.write_all(bufview).unwrap();
                },
                Action::ReadAndCheck(sz) => {
                    let bufview  = &mut buf[0..sz];
                    let bufview2 = &mut buf2[0..sz];
                    let ret1 = c1.read(bufview).unwrap();
                    let ret2 = c2.read(bufview2).unwrap();
                    assert!(ret1 == ret2);
                    assert!(bufview[0..ret1] == bufview2[0..ret2]);
                },
            }
        }
        drop(c2);

        let backing_storage1 = c1.into_inner();
        let backing_storage2 = h.join().unwrap().into_inner();

        //eprintln!("{:?}", &backing_storage1[0..16]);
        //eprintln!("{:?}", &backing_storage2[0..16]);

        assert!(backing_storage1 == backing_storage2);
    }
}
