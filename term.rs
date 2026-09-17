#![allow(non_camel_case_types)]

use std::ffi::{c_int,c_uint,c_uchar};
use std::io;
use std::io::prelude::*;

use std::io::Write; // <--- bring flush() into scope

type cc_t = c_uchar;
type speed_t = c_uint;
type tcflag_t = c_uint;

const NCCS: usize = 32;

#[derive(Debug)]
#[repr(C)] // <-- without this it did not work out
struct Termios {
    c_iflag: tcflag_t,
    c_oflag: tcflag_t,
    c_cflag: tcflag_t,
    c_lflag: tcflag_t,
    c_line: cc_t,
    c_cc: [cc_t; NCCS],
    c_ispeed: speed_t,
    c_ospeed: speed_t
}

unsafe extern "C" {
    fn tcgetattr(fd: c_int, termios_p: *mut Termios) -> c_int;
    fn tcsetattr(fd: c_int, optional_actions: c_int, termios_p: *const Termios) -> c_int;
}

enum Streams {
    STDIN = 0,
    STDOUT,
    STDERR,
}

enum Term_Attr_Optionals {
    TCSANOW = 0,
    TCSADRAIN,
    TCSAFLUSH,
}

fn main() {


    let mut t = Termios {
        c_iflag: 0,
        c_oflag: 0,
        c_cflag: 0,
        c_lflag: 0,
        c_line: 0,
        c_cc: [0;32],
        c_ispeed: 0,
        c_ospeed: 0,
    };

    println!("{:?}", t);
    unsafe {
        tcgetattr(Streams::STDIN as i32, &mut t);
    }
    println!("{:?}", t);

    t.c_lflag &= ! 0b00000000000000000000000000001000; // disable ECHO mode
    //t.c_lflag = 8; // ECHO mode
    
    // read user input in loop
    let mut stdin = io::stdin().lock();
    unsafe {        
        tcsetattr(Streams::STDIN as i32, Term_Attr_Optionals::TCSAFLUSH as i32, &mut t);
    }
    println!("{:?}", t);
    
    let mut byte = [0u8]; // 0 initialized 1-byte array
    // read byte by byte
    loop {
        stdin.read_exact(&mut byte).unwrap();
        print!("{}", byte[0] as char);
        io::stdout().flush().unwrap();
    }

}
