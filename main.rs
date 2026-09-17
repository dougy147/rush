#![allow(non_camel_case_types)]

use std::ffi::{c_int,c_uint,c_uchar};
use std::fs::File;
use std::io;
use std::io::prelude::*;

use std::io::Write; // <--- bring flush() into scope

type cc_t = c_uchar;
type speed_t = c_uint;
type tcflag_t = c_uint;

const NCCS: usize = 32;

#[derive(Debug,Default)] // Default is to allow 0-initialization, Debug to print it with {:?}
#[repr(C)] // <-- without this modifying terminal attributes did not work out
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

struct User_Search {
    bytes: [u8;512],
    size: usize,
}

fn main() -> std::io::Result<()> {

    
    let mut file: File = File::open(history_file_path)?;
    
    let mut history = String::new();
    let size = file.read_to_string(&mut history);

    println!("[x] Read history file \"{}\" (size = {:?})", history_file_path, size);

    // read user input in loop
    //let mut stdin = io::stdin().lock();

    //// loop
    //let mut byte = [0u8]; // 0 initialized 1-byte array
    //// read byte by byte
    //loop {
    //    stdin.read_exact(&mut byte).unwrap();
    //    println!("<{}>", byte[0]);
    //}
   
    let mut t: Termios = Default::default(); // zero initialization

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
    let mut bytes = [0u8;512];

    
    let mut user_search = User_Search {
        bytes: [0;512],
        size: 0,
    };

    // read byte by byte
    loop {
        stdin.read_exact(&mut byte).unwrap();
        //stdin.read_exact(&mut bytes).unwrap();
        
        let search_string = str::from_utf8(&bytes).unwrap().trim_end_matches('\0');
        print!("{:?}", search_string);
        io::stdout().flush().unwrap();

        if byte[0] != 10 { // if not pressed enter add to user_search
            user_search.bytes[user_search.size] = byte[0];
            user_search.size += 1;
        } else {
            let cmds = history.split("\n");
            //let search_string = str::from_utf8(&user_search.bytes).unwrap().trim_end_matches('\0');
            let search_string = str::from_utf8(&bytes).unwrap().trim_end_matches('\0');


            for cmd in cmds {
                if cmd.contains(search_string) {
                    print!("cmd => {}\n", cmd);
                }
            }
            user_search.bytes = [0;512];
            user_search.size = 0;
        }
        //io::stdout().flush().unwrap();
    }
}
