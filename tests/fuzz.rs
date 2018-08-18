#![allow(unused,dead_code)]

#[macro_use]
extern crate proptest;
extern crate rand;
extern crate readwrite;
extern crate pipe;
extern crate nbd;

use rand::prng::XorShiftRng;
use rand::{SeedableRng,RngCore};

use proptest::prelude::{Strategy,any,prop};

use std::io::{Seek,SeekFrom,Read,Write,Cursor};

use readwrite::ReadWrite;

#[derive(Debug,Eq,PartialEq)]
enum Action {
    Seek(u64),
    Write(usize),
    ReadAndCheck(usize),
}

struct Scenario(Vec<Action>);


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
        (0..1024*1024u64).prop_map(Action::Seek),
        biased_size().prop_map(Action::Write),
        biased_size().prop_map(Action::ReadAndCheck),
    }
}

proptest! {
    #[test]
    fn fuzz(script in prop::collection::vec(gen_action(),3..12)) {
        let mut seed = [4u8;16];
        let mut r = XorShiftRng::from_seed(seed);
        
        let mut buf = vec![0;65536];
        let mut buf2 = vec![0;65536];
    
        let backing_storage1 = vec![0u8;1024*1024];
        let backing_storage2 = vec![0u8;1024*1024];
        
        let mut c1 = Cursor::new(backing_storage1);
        let mut c2 = Cursor::new(backing_storage2);
        
        let (r1,w1) = pipe::pipe();
        let (r2,w2) = pipe::pipe();
        let (s1,s2) = (ReadWrite::new(r1,w2), ReadWrite::new(r2,w1));
        
        let h = std::thread::spawn(move || {
            nbd::server::transmission(s2, &mut c2);
            c2
        });
        
        let mut c2 = nbd::client::NbdClient::new(s1, &nbd::Export{size:1024*1024,..Default::default()});
    
        for i in script {
            match i {
                Action::Seek(pos) => {
                    c1.seek(SeekFrom::Start(pos));
                    c2.seek(SeekFrom::Start(pos));
                },
                Action::Write(sz) => {
                    let bufview = &mut buf[0..sz];
                    r.fill_bytes(bufview);
                    c1.write_all(bufview).unwrap();
                    c2.write_all(bufview).unwrap();
                },
                Action::ReadAndCheck(mut sz) => {
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
        assert!(c1.into_inner() == h.join().unwrap().into_inner());
    }
}
