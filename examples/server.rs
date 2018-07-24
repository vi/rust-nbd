#![allow(unused)]
extern crate nbd;

use std::io::{Read,Write,Result};
use std::net::{TcpListener, TcpStream};
use std::thread;

use nbd::server::{handshake, Export, transmission};

fn handle_client(mut stream: TcpStream) -> Result<()> {
    let e = Export {
        size: 1234 * 1024,
        readonly: true,
        ..Default::default()
    };
    handshake(&mut stream, &e)?;
    transmission(&mut stream)?;
    Ok(())
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:10809").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                match handle_client(stream) {
                    Ok(_) => {},
                    Err(e) => {
                        eprintln!("error: {}", e);
                    }
                }
            }
            Err(_) => {
                println!("Error");
            }
        }
    }
}
