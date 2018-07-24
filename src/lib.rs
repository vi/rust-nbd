//! First sketch of NBD (Network block device) protocol support in Rust
//! API is not stable yet, obviously
//!
//! https://github.com/NetworkBlockDevice/nbd/blob/master/doc/proto.md

#![allow(unused)]

extern crate byteorder;

pub mod server {

    use byteorder::{BigEndian as BE, ReadBytesExt, WriteBytesExt};
    use std::io::{Cursor, Error, ErrorKind, Read, Result, Write};

    pub fn oldstyle_header<W: Write>(mut c: W, size: u64, flags: u32) -> Result<()> {
        c.write_all(b"NBDMAGIC")?;
        c.write_all(b"\x00\x42\x02\x81\x86\x12\x53")?;
        c.write_u64::<BE>(size)?;
        c.write_u32::<BE>(flags)?;
        Ok(())
    }

    fn strerror(s: &'static str) -> Result<()> {
        let stderr: Box<::std::error::Error + Send + Sync> = s.into();
        Err(Error::new(ErrorKind::InvalidData, stderr))
    }

    fn reply<IO: Write + Read>(mut c: IO, clopt: u32, rtype: u32, data: &[u8]) -> Result<()> {
        c.write_u64::<BE>(0x3e889045565a9)?;
        c.write_u32::<BE>(clopt)?;
        c.write_u32::<BE>(rtype)?;
        c.write_u32::<BE>(data.len() as u32)?;
        c.write_all(data)?;
        c.flush()?;
        Ok(())
    }

    #[derive(Debug, Default)]
    pub struct Export {
        pub size: u64,
        pub readonly: bool,
        pub resizeable: bool,
        pub rotational: bool,
        pub send_trim: bool,
    }

    pub fn handshake<IO: Write + Read>(mut c: IO, export: &Export) -> Result<()> {
        //let hs_flags = NBD_FLAG_FIXED_NEWSTYLE;
        let hs_flags = NBD_FLAG_FIXED_NEWSTYLE;

        c.write_all(b"NBDMAGIC")?;
        c.write_all(b"IHAVEOPT")?;
        c.write_u16::<BE>(hs_flags);
        c.flush()?;

        let client_flags = c.read_u32::<BE>()?;

        if client_flags != NBD_FLAG_C_FIXED_NEWSTYLE {
            strerror("Invalid client flag")?;
        }

        let client_optmagic = c.read_u64::<BE>()?;

        if client_optmagic != 0x49484156454F5054 {
            // IHAVEOPT
            strerror("Invalid client optmagic")?;
        }

        loop {
            let clopt = c.read_u32::<BE>()?;
            let optlen = c.read_u32::<BE>()?;

            if optlen > 100000 {
                strerror("Suspiciously big option length")?;
            }

            let mut opt = vec![0; optlen as usize];
            c.read_exact(&mut opt)?;

            match clopt {
                NBD_OPT_EXPORT_NAME => {
                    c.write_u64::<BE>(export.size)?;
                    let mut flags = NBD_FLAG_HAS_FLAGS;
                    if export.readonly {
                        flags |= NBD_FLAG_READ_ONLY
                    };
                    if export.resizeable {
                        flags |= NBD_FLAG_READ_ONLY
                    };
                    if export.rotational {
                        flags |= NBD_FLAG_ROTATIONAL
                    };
                    if export.send_trim {
                        flags |= NBD_FLAG_SEND_TRIM
                    };
                    c.write_u16::<BE>(flags)?;
                    c.write_all(&[0; 124])?;
                    return Ok(());
                }
                NBD_OPT_ABORT => {
                    reply(&mut c, clopt, NBD_REP_ACK, b"")?;
                    strerror("Client abort")?;
                }
                NBD_OPT_LIST => {
                    if optlen != 0 {
                        strerror("NBD_OPT_LIST with content")?;
                    }

                    reply(&mut c, clopt, NBD_REP_SERVER, b"\x00\x00\x00\x07rustnbd")?;
                    reply(&mut c, clopt, NBD_REP_ACK, b"")?;
                }
                NBD_OPT_STARTTLS => {
                    strerror("TLS not supported")?;
                }
                NBD_OPT_INFO => {
                    reply(&mut c, clopt, NBD_REP_ERR_UNSUP, b"");
                }
                NBD_OPT_GO => {
                    reply(&mut c, clopt, NBD_REP_ERR_UNSUP, b"");
                }
                _ => {
                    strerror("Invalid client option type");
                }
            }
        }
    }

    pub const DEFAULT_TCP_PORT: u16 = 10809;

    const NBD_OPT_EXPORT_NAME: u32 = 1;
    const NBD_OPT_ABORT: u32 = 2;
    const NBD_OPT_LIST: u32 = 3;
    const NBD_OPT_STARTTLS: u32 = 5;
    const NBD_OPT_INFO: u32 = 6;
    const NBD_OPT_GO: u32 = 7;

    const NBD_REP_ACK: u32 = 1;
    const NBD_REP_SERVER: u32 = 2;
    const NBD_REP_INFO: u32 = 3;
    const NBD_REP_FLAG_ERROR: u32 = (1 << 31);
    const NBD_REP_ERR_UNSUP: u32 = (1 | NBD_REP_FLAG_ERROR);
    const NBD_REP_ERR_POLICY: u32 = (2 | NBD_REP_FLAG_ERROR);
    const NBD_REP_ERR_INVALID: u32 = (3 | NBD_REP_FLAG_ERROR);
    const NBD_REP_ERR_PLATFORM: u32 = (4 | NBD_REP_FLAG_ERROR);
    const NBD_REP_ERR_TLS_REQD: u32 = (5 | NBD_REP_FLAG_ERROR);
    const NBD_REP_ERR_UNKNOWN: u32 = (6 | NBD_REP_FLAG_ERROR);
    const NBD_REP_ERR_BLOCK_SIZE_REQD: u32 = (8 | NBD_REP_FLAG_ERROR);

    const NBD_FLAG_FIXED_NEWSTYLE: u16 = (1 << 0);
    const NBD_FLAG_NO_ZEROES: u16 = (1 << 1);

    const NBD_FLAG_C_FIXED_NEWSTYLE: u32 = NBD_FLAG_FIXED_NEWSTYLE as u32;
    const NBD_FLAG_C_NO_ZEROES: u32 = NBD_FLAG_NO_ZEROES as u32;

    const NBD_INFO_EXPORT: u16 = 0;
    const NBD_INFO_NAME: u16 = 1;
    const NBD_INFO_DESCRIPTION: u16 = 2;
    const NBD_INFO_BLOCK_SIZE: u16 = 3;

    const NBD_FLAG_HAS_FLAGS: u16 = (1 << 0);
    const NBD_FLAG_READ_ONLY: u16 = (1 << 1);
    const NBD_FLAG_SEND_FLUSH: u16 = (1 << 2);
    const NBD_FLAG_SEND_FUA: u16 = (1 << 3);
    const NBD_FLAG_ROTATIONAL: u16 = (1 << 4);
    const NBD_FLAG_SEND_TRIM: u16 = (1 << 5);
    const NBD_FLAG_SEND_WRITE_ZEROES: u16 = (1 << 6);
    const NBD_FLAG_CAN_MULTI_CONN: u16 = (1 << 8);

} // mod server

/*
// Options that the client can select to the server 
#define NBD_OPT_EXPORT_NAME     (1)     // Client wants to select a named export (is followed by name of export) 
#define NBD_OPT_ABORT           (2)     // Client wishes to abort negotiation 
#define NBD_OPT_LIST            (3)     // Client request list of supported exports (not followed by data) 
#define NBD_OPT_STARTTLS        (5)     // Client wishes to initiate TLS 
#define NBD_OPT_INFO            (6)     // Client wants information about the given export 
#define NBD_OPT_GO              (7)     // Client wants to select the given and move to the transmission phase 

// Replies the server can send during negotiation 
#define NBD_REP_ACK             (1)     // ACK a request. Data: option number to be acked 
#define NBD_REP_SERVER          (2)     // Reply to NBD_OPT_LIST (one of these per server; must be followed by NBD_REP_ACK to signal the end of the list 
#define NBD_REP_INFO            (3)     // Reply to NBD_OPT_INFO 
#define NBD_REP_FLAG_ERROR      (1 << 31)       // If the high bit is set, the reply is an error 
#define NBD_REP_ERR_UNSUP       (1 | NBD_REP_FLAG_ERROR)        // Client requested an option not understood by this version of the server 
#define NBD_REP_ERR_POLICY      (2 | NBD_REP_FLAG_ERROR)        // Client requested an option not allowed by server configuration. (e.g., the option was disabled) 
#define NBD_REP_ERR_INVALID     (3 | NBD_REP_FLAG_ERROR)        // Client issued an invalid request 
#define NBD_REP_ERR_PLATFORM    (4 | NBD_REP_FLAG_ERROR)        // Option not supported on this platform 
#define NBD_REP_ERR_TLS_REQD    (5 | NBD_REP_FLAG_ERROR)        // TLS required 
#define NBD_REP_ERR_UNKNOWN     (6 | NBD_REP_FLAG_ERROR)        // NBD_OPT_INFO or ..._GO requested on unknown export 
#define NBD_REP_ERR_BLOCK_SIZE_REQD (8 | NBD_REP_FLAG_ERROR)    // Server is not willing to serve the export without the block size being negotiated 

// Global flags 
#define NBD_FLAG_FIXED_NEWSTYLE (1 << 0)        // new-style export that actually supports extending 
#define NBD_FLAG_NO_ZEROES      (1 << 1)        // we won't send the 128 bits of zeroes if the client sends NBD_FLAG_C_NO_ZEROES 
// Flags from client to server. 
#define NBD_FLAG_C_FIXED_NEWSTYLE NBD_FLAG_FIXED_NEWSTYLE
#define NBD_FLAG_C_NO_ZEROES    NBD_FLAG_NO_ZEROES

// Info types 
#define NBD_INFO_EXPORT         (0)
#define NBD_INFO_NAME           (1)
#define NBD_INFO_DESCRIPTION    (2)
#define NBD_INFO_BLOCK_SIZE     (3)

// values for flags field
#define NBD_FLAG_HAS_FLAGS      (1 << 0)        // Flags are there 
#define NBD_FLAG_READ_ONLY      (1 << 1)        // Device is read-only 
#define NBD_FLAG_SEND_FLUSH     (1 << 2)        // Send FLUSH 
#define NBD_FLAG_SEND_FUA       (1 << 3)        // Send FUA (Force Unit Access) 
#define NBD_FLAG_ROTATIONAL     (1 << 4)        // Use elevator algorithm - rotational media 
#define NBD_FLAG_SEND_TRIM      (1 << 5)        // Send TRIM (discard) 
#define NBD_FLAG_SEND_WRITE_ZEROES (1 << 6)     // Send NBD_CMD_WRITE_ZEROES 
#define NBD_FLAG_CAN_MULTI_CONN (1 << 8)        // multiple connections are okay 


*/

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
