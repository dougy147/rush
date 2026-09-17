#![allow(non_camel_case_types)]

use std::ffi::{c_int,c_uint,c_uchar};
use std::fs::File;
use std::io;
use std::io::prelude::*;

//use std::io::Write; // <--- bring flush() into scope

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
    //fn readline(prompt: *const c_uchar) -> *mut c_uchar;
}

struct User_Search {
    bytes: [u8;512],
    size: usize,
}

const STDIN     : c_int = 0;
const TCSAFLUSH : c_int = 2;

const ECHO   : tcflag_t = 0b1000;
const ICANON : tcflag_t = 0b100;

fn save_terminal(terminal: &mut Termios) -> () {
    unsafe { tcgetattr(STDIN, terminal); }
}

fn restore_terminal(terminal: &mut Termios) -> () {
    unsafe { tcsetattr(STDIN, TCSAFLUSH, terminal); }
}

const CTRL_C: u8 = 3;
const ENTER: u8 = 10;
const ESC: u8 = 27;
const BACKSPACE: u8 = 127;

fn set_terminal_raw_mode(terminal: &mut Termios) -> () {
    terminal.c_lflag &= ICANON; // enable ICANON  raw mode
    terminal.c_lflag &= ! ECHO ; // disable ECHO mode
    unsafe { tcsetattr(STDIN, TCSAFLUSH, terminal); }
}

fn main() -> std::io::Result<()> {

    
    let mut file: File = File::open(history_file_path)?;
    
    let mut history = String::new();
    let size = file.read_to_string(&mut history);

    println!("[x] Read history file \"{}\" (size = {:?})", history_file_path, size);
    
    let mut terminal_at_start: Termios = Default::default();
    
    save_terminal(&mut terminal_at_start);
    
    let mut t: Termios = Default::default();
    unsafe {
        tcgetattr(STDIN, &mut t)
    };

    set_terminal_raw_mode(&mut t);
    
    let mut raw_key = [0u8;1]; // read 1 bytes from stdin
    // later we will read more bytes depending on this one
    // this is useful because Alt+f or whatever is Esc+f so 2 bytes ()
    // check here for more shortcuts : https://github.com/mateolafalce/k_board/blob/main/src/keys.rs
    
    let mut user_search = User_Search {
        bytes: [0;512],
        size: 0,
    };

    // read user input in loop
    let mut stdin = io::stdin().lock();

    let mut escape_mode: bool = false;
    
    //read byte by byte
    loop {
        stdin.read_exact(&mut raw_key).unwrap();
        let key = raw_key[0];

        // TODO: which key was pressed?
        match key {
            
            CTRL_C => break, // Ctrl+c
            
            ESC => {
                escape_mode = true;
                continue;
            },
            
            _ => {
                if !escape_mode {
                    if key == ENTER {
                        let cmds = history.split("\n");
                        let search_string = str::from_utf8(&user_search.bytes)
                            .unwrap()
                            .trim_end_matches('\0');
                        
                        for cmd in cmds {
                            if cmd.contains(search_string) {
                                print!("cmd => {}\n", cmd);
                            }
                        }
                        user_search.bytes = [0;512];
                        user_search.size = 0;

                    } else {
                        user_search.bytes[user_search.size] = key;
                        user_search.size += 1;
                        print!("search = {} ; stdin = {:?}\n", str::from_utf8(&user_search.bytes).unwrap().trim_end_matches('\0'), raw_key);
                    }
                } else {
                    escape_mode = false;
                    if key == b'b' {
                        println!("alt+b");
                    } else if key == b'f' {
                        println!("alt+f");
                    } else if key == BACKSPACE {
                        println!("alt+backspace");                        
                    }
                }
            },
            
        }
        //io::stdout().flush().unwrap();
    }

    restore_terminal(&mut terminal_at_start);
    Ok(())
}
