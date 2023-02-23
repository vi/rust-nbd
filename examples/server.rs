extern crate nbd;

use std::io::{Cursor, Result};
use std::net::{TcpListener, TcpStream};

use nbd::server::{handshake, transmission, Export};

fn handle_client(mut stream: TcpStream) -> Result<()> {
    let data = handshake(&mut stream, |name| {
        println!("requested export: {name}");
        let mut data = vec![0; 1_474_560];
        let signature = format!("Name of the export requested by client: `{}`.", name).into_bytes();
        data[0..signature.len()].copy_from_slice(&signature);
        Ok(Export {
            size: data.len() as u64,
            data,
            readonly: false,
            ..Default::default()
        })
    })?;
    let pseudofile = Cursor::new(data);
    transmission(&mut stream, pseudofile)?;
    Ok(())
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:10809").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => match handle_client(stream) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("error: {}", e);
                }
            },
            Err(_) => {
                println!("Error");
            }
        }
    }
}
