#![allow(unused)]
extern crate nbd;

use std::io::Read;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::thread;

use nbd::server::{handshake, Export};

fn handle_client(mut stream: TcpStream) {
    let e = Export {
        size: 1234 * 1024,
        readonly: true,
        ..Default::default()
    };
    match handshake(&mut stream, &e) {
        Ok(_) => {
            eprintln!("Tranmission not implemented");
        }
        Err(e) => {
            eprintln!("error: {}", e);
        }
    }
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:10809").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                handle_client(stream);
            }
            Err(_) => {
                println!("Error");
            }
        }
    }
}
