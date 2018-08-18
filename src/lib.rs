//! First sketch of NBD (Network block device) protocol support in Rust
//! API is not stable yet, obviously
//!
//! https://github.com/NetworkBlockDevice/nbd/blob/master/doc/proto.md

#![deny(missing_docs)]
#![forbid(unsafe_code)]

extern crate byteorder;

/// Information about an export (without name)
#[derive(Debug, Default, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct Export {
    /// Size of the underlying data, in bytes
    pub size: u64,
    /// Tell client it's readonly
    pub readonly: bool,
    /// Tell that NBD_CMD_RESIZE should be supported. Not implemented in this library currently
    pub resizeable: bool,
    /// Tell that the exposed device has slow seeks, hence clients should use elevator algorithm
    pub rotational: bool,
    /// Tell that NBD_CMD_TRIM operation is supported. Not implemented in this library currently
    pub send_trim: bool,
    /// Tell that NBD_CMD_FLUSH may be sent
    pub send_flush: bool,
}

fn strerror(s: &'static str) -> std::io::Result<()> {
    Err(std::io::Error::new(std::io::ErrorKind::InvalidData, s))
}

// based on https://doc.rust-lang.org/src/std/io/util.rs.html#48
fn mycopy<R: ?Sized, W: ?Sized>(
    reader: &mut R,
    writer: &mut W,
    buf: &mut [u8],
    mut limit: usize,
) -> ::std::io::Result<u64>
where
    R: ::std::io::Read,
    W: ::std::io::Write,
{
    let mut written = 0;
    loop {
        let to_read = buf.len().min(limit);
        let len = match reader.read(&mut buf[0..to_read]) {
            Ok(0) => return Ok(written),
            Ok(len) => len,
            Err(ref e) if e.kind() == ::std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        writer.write_all(&buf[..len])?;
        written += len as u64;
        //eprintln!("written={} limit={} len={}", written, limit, len);
        limit -= len;
        if limit == 0 {
            return Ok(written);
        }
    }
}

/// Items for implementing NBD server
///
/// "Serialize" your Read+Write+Seek into a Read+Write socket using standard protocol.
pub mod server {

    use super::consts::*;
    use super::{mycopy, strerror};
    use byteorder::{BigEndian as BE, ReadBytesExt, WriteBytesExt};
    use std::io::{Error, Read, Result, Seek, SeekFrom, Write};

    #[doc(hidden)]
    pub fn oldstyle_header<W: Write>(mut c: W, size: u64, flags: u32) -> Result<()> {
        c.write_all(b"NBDMAGIC")?;
        c.write_all(b"\x00\x00\x42\x02\x81\x86\x12\x53")?;
        c.write_u64::<BE>(size)?;
        c.write_u32::<BE>(flags)?;
        c.write_all(&[0; 124])?;
        c.flush()?;
        Ok(())
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

    pub use super::Export;

    /// Ignores incoming export name, accepts everything
    /// Export name is ignored, currently only one export is supported
    pub fn handshake<IO: Write + Read>(mut c: IO, export: &Export) -> Result<()> {
        //let hs_flags = NBD_FLAG_FIXED_NEWSTYLE;
        let hs_flags = NBD_FLAG_FIXED_NEWSTYLE;

        c.write_all(b"NBDMAGIC")?;
        c.write_all(b"IHAVEOPT")?;
        c.write_u16::<BE>(hs_flags)?;
        c.flush()?;

        let client_flags = c.read_u32::<BE>()?;

        if client_flags != NBD_FLAG_C_FIXED_NEWSTYLE {
            strerror("Invalid client flag")?;
        }

        loop {
            let client_optmagic = c.read_u64::<BE>()?;

            if client_optmagic != 0x49484156454F5054 {
                // IHAVEOPT
                strerror("Invalid client optmagic")?;
            }

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
                    } else {
                        flags |= NBD_FLAG_SEND_FLUSH
                    };
                    if export.resizeable {
                        flags |= NBD_FLAG_SEND_RESIZE
                    };
                    if export.rotational {
                        flags |= NBD_FLAG_ROTATIONAL
                    };
                    if export.send_trim {
                        flags |= NBD_FLAG_SEND_TRIM
                    };
                    c.write_u16::<BE>(flags)?;
                    c.write_all(&[0; 124])?;
                    c.flush()?;
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
                    reply(&mut c, clopt, NBD_REP_ERR_UNSUP, b"")?;
                }
                NBD_OPT_GO => {
                    reply(&mut c, clopt, NBD_REP_ERR_UNSUP, b"")?;
                }
                _ => {
                    strerror("Invalid client option type")?;
                }
            }
        }
    }

    fn replyt<IO: Write + Read>(mut c: IO, error: u32, handle: u64) -> Result<()> {
        c.write_u32::<BE>(0x67446698)?;
        c.write_u32::<BE>(error)?;
        c.write_u64::<BE>(handle)?;
        Ok(())
    }

    fn replyte<IO: Write + Read>(mut c: IO, error: Error, handle: u64) -> Result<()> {
        let ec = if let Some(x) = error.raw_os_error() {
            if (x as u32) != 0 {
                x as u32
            } else {
                5
            }
        } else {
            5
        };
        replyt(&mut c, ec, handle)
    }

    /// Serve given data. If readonly, use a dummy `Write` implementation.
    ///
    /// Should be used after `handshake`
    pub fn transmission<IO, D>(mut c: IO, mut data: D) -> Result<()>
    where
        IO: Read + Write,
        D: Read + Write + Seek,
    {
        let mut buf = vec![0; 65536];
        loop {
            let magic = c.read_u32::<BE>()?;
            if magic != 0x25609513 {
                strerror("Invalid request magic")?;
            }
            let _flags = c.read_u16::<BE>()?;
            let typ = c.read_u16::<BE>()?;
            let handle = c.read_u64::<BE>()?;
            let offset = c.read_u64::<BE>()?;
            let length = c.read_u32::<BE>()?;

            //eprintln!("typ={} handle={} off={} len={}", typ, handle, offset, length);
            match typ {
                NBD_CMD_READ => {
                    if let Err(e) = data.seek(SeekFrom::Start(offset)) {
                        replyte(&mut c, e, handle)?;
                    } else {
                        replyt(&mut c, 0, handle)?;
                        match mycopy(&mut data, &mut c, &mut buf, length as usize) {
                            Err(e) => replyte(&mut c, e, handle)?,
                            Ok(x) if x == (length as u64) => {}
                            Ok(_) => {
                                strerror("sudden EOF")?;
                            }
                        }
                    }
                }
                NBD_CMD_WRITE => {
                    if let Err(e) = data.seek(SeekFrom::Start(offset)) {
                        replyte(&mut c, e, handle)?;
                    } else {
                        match mycopy(&mut c, &mut data, &mut buf, length as usize) {
                            Err(e) => replyte(&mut c, e, handle)?,
                            Ok(x) if x == (length as u64) => {
                                replyt(&mut c, 0, handle)?;
                            }
                            Ok(_) => {
                                strerror("sudden EOF")?;
                            }
                        }
                    }
                }
                NBD_CMD_DISC => {
                    return Ok(());
                }
                NBD_CMD_FLUSH => {
                    data.flush()?;
                    replyt(&mut c, 0, handle)?;
                }
                NBD_CMD_TRIM => {
                    replyt(&mut c, 38, handle)?;
                }
                NBD_CMD_WRITE_ZEROES => {
                    replyt(&mut c, 38, handle)?;
                }
                _ => strerror("Unknown command from client")?,
            }
            c.flush()?;
        }
    }

    /// Recommended port for NBD servers, especially with new handshake format.
    /// There is some untested, doc-hidden old handshake support in this library.
    pub const DEFAULT_TCP_PORT: u16 = 10809;

} // mod server

/// Items for implementing NBD client.
///
/// Turn Read+Write into a Read+Write+Seek using a standard protocol.
pub mod client {
    use super::consts::*;
    use super::{strerror, CheckedAddI64, ClampToU32};
    use byteorder::{BigEndian as BE, ReadBytesExt, WriteBytesExt};
    use std::io::{Error, ErrorKind, Read, Result, Seek, SeekFrom, Write};

    pub use super::Export;

    fn fill_in_flags(export: &mut Export, flags: u16) {
        if flags & NBD_FLAG_HAS_FLAGS != 0 {
            if flags & NBD_FLAG_READ_ONLY != 0 {
                export.readonly = true;
            }
            if flags & NBD_FLAG_SEND_RESIZE != 0 {
                export.resizeable = true;
            }
            if flags & NBD_FLAG_ROTATIONAL != 0 {
                export.rotational = true;
            }
            if flags & NBD_FLAG_SEND_TRIM != 0 {
                export.send_trim = true;
            }
            if flags & NBD_FLAG_SEND_FLUSH != 0 {
                export.send_flush = true;
            }
        }
    }

    /// Negotiate with a server, use before creating the actual client
    pub fn handshake<IO: Write + Read>(mut c: IO, name: &[u8]) -> Result<Export> {
        let mut signature = [0; 8];
        c.read_exact(&mut signature)?;

        if signature != *b"NBDMAGIC" {
            strerror("Invalid magic1")?;
        }

        c.read_exact(&mut signature)?;

        let (size, flags) = if signature == *b"IHAVEOPT" {
            // newstyle
            let _hs_flags = c.read_u16::<BE>()?;

            c.write_u32::<BE>(NBD_FLAG_C_FIXED_NEWSTYLE)?;

            // optmagic
            c.write_u64::<BE>(0x49484156454F5054)?;
            c.write_u32::<BE>(NBD_OPT_EXPORT_NAME)?;
            c.write_u32::<BE>(name.len() as u32)?;
            c.write_all(name)?;
            c.flush()?;

            let size = c.read_u64::<BE>()?;
            let flags = c.read_u16::<BE>()?;
            let mut z = [0; 124];
            c.read_exact(&mut z)?;
            if z[..] != [0; 124][..] {
                strerror("Expected 124 bytes of zeroes are not zeroes")?;
            }
            (size, flags)
        } else if signature == *b"\x00\x00\x42\x02\x81\x86\x12\x53" {
            // oldstyle.
            // Note: not tested at all
            if name != b"" {
                strerror("Old style server does not support named exports")?;
            };
            let size = c.read_u64::<BE>()?;
            let flags = c.read_u32::<BE>()?;
            let mut z = [0; 124];
            c.read_exact(&mut z)?;
            if z[..] != [0; 124][..] {
                strerror("Expected 124 bytes of zeroes are not zeroes")?;
            }

            // Is it those flags or some other flags?
            // Too lazy to actually look into NBD implementation.
            let flags = flags as u16;

            (size, flags)
        } else {
            strerror("Invalid magic2")?;
            unreachable!()
        };

        let mut e = Export::default();
        e.size = size;

        fill_in_flags(&mut e, flags);

        Ok(e)
    }

    /// Represents NBD client. Use `Read`,`Write` and `Seek` trait methods,
    /// but make sure those are block-aligned
    pub struct NbdClient<IO: Write + Read> {
        c: IO,
        seek_pos: u64,
        size: u64,
    }

    impl<IO: Write + Read> NbdClient<IO> {
        /// Create new NbdClient from `Export` returned from `handshake`.
        /// Obviously, the `c` connection should be the same as in `handshake`.
        pub fn new(c: IO, export: &Export) -> Self {
            NbdClient {
                c,
                seek_pos: 0,
                size: export.size,
            }
        }
    }

    impl<IO: Write + Read> Seek for NbdClient<IO> {
        fn seek(&mut self, sf: SeekFrom) -> Result<u64> {
            match sf {
                SeekFrom::Start(x) => {
                    self.seek_pos = x;
                }
                SeekFrom::Current(x) => {
                    if let Some(xx) = self.seek_pos.checked_add_i64(x) {
                        self.seek_pos = xx;
                    } else {
                        strerror("Invalid seek")?;
                    }
                }
                SeekFrom::End(x) => {
                    if let Some(xx) = self.size.checked_add_i64(x) {
                        self.seek_pos = xx;
                    } else {
                        strerror("Invalid seek")?;
                    }
                }
            }
            Ok(self.seek_pos)
        }
    }

    fn check_err(error: u32) -> Result<()> {
        match error {
            1 => Err(Error::new(ErrorKind::PermissionDenied, "from device")),
            5 => Err(Error::new(ErrorKind::Other, "EIO")),
            12 => Err(Error::new(ErrorKind::Other, "ENOMEM")),
            22 => Err(Error::new(ErrorKind::Other, "EINVAL")),
            28 => Err(Error::new(ErrorKind::Other, "ENOSPC")),
            0 => Ok(()),
            _ => Err(Error::new(ErrorKind::Other, "other error from device")),
        }
    }

    fn getreply<IO: Write + Read>(mut c: IO) -> Result<()> {
        let signature = c.read_u32::<BE>()?;
        let error = c.read_u32::<BE>()?;
        let handle = c.read_u64::<BE>()?;

        if signature != 0x67446698 {
            strerror("Invalid signature for incoming reply")?;
        }
        if handle != 0 {
            strerror("Unexpected handle")?;
        };
        check_err(error)?;
        Ok(())
    }

    fn sendrequest<IO: Write + Read>(mut c: IO, cmd: u16, offset: u64, len: u32) -> Result<()> {
        c.write_u32::<BE>(0x25609513)?;
        c.write_u16::<BE>(0)?; // flags
        c.write_u16::<BE>(cmd)?;
        c.write_u64::<BE>(0)?; // handle
        c.write_u64::<BE>(offset)?;
        c.write_u32::<BE>(len)?;
        c.flush()?;
        Ok(())
    }

    impl<IO: Write + Read> NbdClient<IO> {
        fn get_effective_len(&self, buflen: usize) -> Result<u32> {
            if self.seek_pos == self.size {
                return Ok(0);
            }
            if self.seek_pos > self.size {
                strerror("Trying to read or write past the end of the device")?;
            }

            let maxlen = (self.size - self.seek_pos).clamp_to_u32();
            let len = buflen.clamp_to_u32().min(maxlen);
            Ok(len)
        }
    }

    impl<IO: Write + Read> Read for NbdClient<IO> {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
            let len = self.get_effective_len(buf.len())?;
            if len == 0 {
                return Ok(0);
            }

            sendrequest(&mut self.c, NBD_CMD_READ, self.seek_pos, len)?;

            getreply(&mut self.c)?;

            self.c.read_exact(&mut buf[0..(len as usize)])?;
            self.seek_pos += len as u64;
            Ok(len as usize)
        }
    }

    impl<IO: Write + Read> Write for NbdClient<IO> {
        fn write(&mut self, buf: &[u8]) -> Result<usize> {
            let len = self.get_effective_len(buf.len())?;
            if len == 0 {
                return Ok(0);
            }

            sendrequest(&mut self.c, NBD_CMD_WRITE, self.seek_pos, len)?;

            self.c.write_all(&buf[0..(len as usize)])?;
            self.c.flush()?;

            getreply(&mut self.c)?;
            self.seek_pos += len as u64;
            Ok(len as usize)
        }
        fn flush(&mut self) -> Result<()> {
            sendrequest(&mut self.c, NBD_CMD_FLUSH, 0, 0)?;
            getreply(&mut self.c)?;
            Ok(())
        }
    }

    /// Additional operations (apart from reading and writing) supported by NBD extensions
    /// Not tested yet.
    pub trait NbdExt {
        /// Discard this data, starting from current seek offset up to specified length
        fn trim(&mut self, length: usize) -> Result<()>;

        /// Change size of the device
        fn resize(&mut self, newsize: u64) -> Result<()>;
    }

    impl<IO: Write + Read> NbdExt for NbdClient<IO> {
        fn trim(&mut self, length: usize) -> Result<()> {
            let len = self.get_effective_len(length)?;
            if len == 0 {
                return Ok(());
            }

            sendrequest(&mut self.c, NBD_CMD_TRIM, self.seek_pos, len)?;

            getreply(&mut self.c)?;

            Ok(())
        }

        fn resize(&mut self, newsize: u64) -> Result<()> {
            sendrequest(&mut self.c, NBD_CMD_RESIZE, newsize, 0)?;

            getreply(&mut self.c)?;
            self.size = newsize;
            Ok(())
        }
    }
}

#[allow(dead_code)]
mod consts {
    pub const NBD_OPT_EXPORT_NAME: u32 = 1;
    pub const NBD_OPT_ABORT: u32 = 2;
    pub const NBD_OPT_LIST: u32 = 3;
    pub const NBD_OPT_STARTTLS: u32 = 5;
    pub const NBD_OPT_INFO: u32 = 6;
    pub const NBD_OPT_GO: u32 = 7;

    pub const NBD_REP_ACK: u32 = 1;
    pub const NBD_REP_SERVER: u32 = 2;
    pub const NBD_REP_INFO: u32 = 3;
    pub const NBD_REP_FLAG_ERROR: u32 = (1 << 31);
    pub const NBD_REP_ERR_UNSUP: u32 = (1 | NBD_REP_FLAG_ERROR);
    pub const NBD_REP_ERR_POLICY: u32 = (2 | NBD_REP_FLAG_ERROR);
    pub const NBD_REP_ERR_INVALID: u32 = (3 | NBD_REP_FLAG_ERROR);
    pub const NBD_REP_ERR_PLATFORM: u32 = (4 | NBD_REP_FLAG_ERROR);
    pub const NBD_REP_ERR_TLS_REQD: u32 = (5 | NBD_REP_FLAG_ERROR);
    pub const NBD_REP_ERR_UNKNOWN: u32 = (6 | NBD_REP_FLAG_ERROR);
    pub const NBD_REP_ERR_BLOCK_SIZE_REQD: u32 = (8 | NBD_REP_FLAG_ERROR);

    pub const NBD_FLAG_FIXED_NEWSTYLE: u16 = (1 << 0);
    pub const NBD_FLAG_NO_ZEROES: u16 = (1 << 1);

    pub const NBD_FLAG_C_FIXED_NEWSTYLE: u32 = NBD_FLAG_FIXED_NEWSTYLE as u32;
    pub const NBD_FLAG_C_NO_ZEROES: u32 = NBD_FLAG_NO_ZEROES as u32;

    pub const NBD_INFO_EXPORT: u16 = 0;
    pub const NBD_INFO_NAME: u16 = 1;
    pub const NBD_INFO_DESCRIPTION: u16 = 2;
    pub const NBD_INFO_BLOCK_SIZE: u16 = 3;

    pub const NBD_FLAG_HAS_FLAGS: u16 = (1 << 0);
    pub const NBD_FLAG_READ_ONLY: u16 = (1 << 1);
    pub const NBD_FLAG_SEND_FLUSH: u16 = (1 << 2);
    pub const NBD_FLAG_SEND_FUA: u16 = (1 << 3);
    pub const NBD_FLAG_ROTATIONAL: u16 = (1 << 4);
    pub const NBD_FLAG_SEND_TRIM: u16 = (1 << 5);
    pub const NBD_FLAG_SEND_WRITE_ZEROES: u16 = (1 << 6);
    pub const NBD_FLAG_CAN_MULTI_CONN: u16 = (1 << 8);
    pub const NBD_FLAG_SEND_RESIZE: u16 = (1 << 9);

    pub const NBD_CMD_READ: u16 = 0;
    pub const NBD_CMD_WRITE: u16 = 1;
    pub const NBD_CMD_DISC: u16 = 2;
    pub const NBD_CMD_FLUSH: u16 = 3;
    pub const NBD_CMD_TRIM: u16 = 4;
    pub const NBD_CMD_WRITE_ZEROES: u16 = 6;
    pub const NBD_CMD_RESIZE: u16 = 8;
}

trait CheckedAddI64
where
    Self: Sized,
{
    fn checked_add_i64(self, rhs: i64) -> Option<Self>;
}
impl CheckedAddI64 for u64 {
    fn checked_add_i64(self, rhs: i64) -> Option<Self> {
        if rhs >= 0 {
            self.checked_add(rhs as u64)
        } else {
            if let Some(x) = rhs.checked_neg() {
                self.checked_sub(x as u64)
            } else {
                None
            }
        }
    }
}
trait ClampToU32
where
    Self: Sized,
{
    fn clamp_to_u32(self) -> u32;
}
impl ClampToU32 for usize {
    fn clamp_to_u32(self) -> u32 {
        if self > u32::max_value() as usize {
            u32::max_value()
        } else {
            self as u32
        }
    }
}
impl ClampToU32 for u64 {
    fn clamp_to_u32(self) -> u32 {
        if self > u32::max_value() as u64 {
            u32::max_value()
        } else {
            self as u32
        }
    }
}

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
