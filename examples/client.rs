extern crate nbd;

use std::io::{Result, Read, Write, Seek, SeekFrom};
use std::net::{TcpStream};

use nbd::client::{handshake, NbdClient};

/// Read second 1024-byte block from an NBD export named "sda1"
fn run() -> Result<()> {
    let mut buf = vec![0;1024];
    
    let mut tcp = TcpStream::connect("127.0.0.1:10809")?;
    let export = handshake(&mut tcp, b"sda1")?;
    let mut client = NbdClient::new(&mut tcp, &export);
    
    client.seek(SeekFrom::Start(1024))?;
    client.read_exact(&mut buf[..])?;
    std::io::stdout().write_all(&buf[..])?;
    
    Ok(())
}

fn main() {
    run().unwrap();
}
