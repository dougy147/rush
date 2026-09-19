#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

use std::ffi::{c_int,c_uint,c_uchar,c_ulong};
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::env;

use std::collections::HashMap;

use std::process::Command;

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

    // use this to inject text to terminal
    //fn ioctl(fd: c_int, op: c_ulong, ...) -> c_int;
}

struct User_Input {
    bytes: [u8;512],
    size: usize,
    cursor: usize, // cursor position
}

/* streams */
const STDIN: c_int = 0;

/* terminal attributes */
const TCSAFLUSH: c_int = 2;

/* terminal flags */
const ECHO:   tcflag_t = 0b1000;
const ICANON: tcflag_t = 0b100;

/* key codes */
const CTRL_c: u8 = 3;
const ENTER: u8 = 10;
const CARRIAGE: u8 = 13;
const ESC: u8 = 27;
const BACKSPACE: u8 = 127;

/* user input cursor moves */
const CTRL_a: u8 = 1; // beginning of line
const CTRL_e: u8 = 5; // end of line
const CTRL_b: u8 = 2; // backward cursor
const CTRL_f: u8 = 6; // forward cursor
const CTRL_n: u8 = 14; // down row
const CTRL_p: u8 = 16; // up row

/* settigns */
const MAX_ROWS: usize = 10; // display a maximum of 10 matching lines

fn hide_cursor() -> () {
    print!("\x1b[?25l");
}

fn show_cursor() -> () {
    print!("\x1b[?25h");
}

fn save_terminal(terminal: &mut Termios) -> () {
    unsafe { tcgetattr(STDIN, terminal); }
}

fn restore_terminal(terminal: &mut Termios) -> () {
    unsafe { tcsetattr(STDIN, TCSAFLUSH, terminal); }
    show_cursor();
}

fn set_terminal_raw_mode(terminal: &mut Termios) -> () {
    terminal.c_lflag &= ICANON; // enable ICANON  raw mode
    terminal.c_lflag &= ! ECHO ; // disable ECHO mode
    unsafe { tcsetattr(STDIN, TCSAFLUSH, terminal); }
    hide_cursor();
}

fn erase_current_output(rows: usize) -> () {
    if rows != 0 {
        print!("\x1B[{}K", rows); // erase matching cmd rows + user input
        print!("\x1B[{}A\x1B[0J", rows); // move cursor up and clean up lines
    }
    print!("\x1B[{}G", 0); // cursor to bol
    print!("\x1B[0K\r");
}

fn delete_backward(user_input: &mut User_Input) {
    // NOTE: we need to null terminate '\0'
    if user_input.cursor > 0 {
        for i in user_input.cursor..user_input.size {
            user_input.bytes[i-1] = user_input.bytes[i];
        }
        user_input.size -= 1;
        user_input.cursor -= 1;
        user_input.bytes[user_input.size] = b'\0';
    }
}

fn insert_key(user_input: &mut User_Input, key: u8) {
    if user_input.cursor != user_input.size {
        // todo: assert no overflow
        for i in (user_input.cursor..user_input.size).rev() {
            user_input.bytes[i+1] = user_input.bytes[i];
        }        
    }
    user_input.bytes[user_input.cursor] = key;
    user_input.size += 1;
    user_input.cursor += 1;
    user_input.bytes[user_input.size] = b'\0';
}

fn cursor_forward(user_input: &mut User_Input) {
    if user_input.cursor < user_input.size {
        user_input.cursor += 1;
    }
}

fn cursor_backward(user_input: &mut User_Input) {
    if user_input.cursor > 0 {
        user_input.cursor -= 1;
    }
}

fn cursor_to_bol(user_input: &mut User_Input) {
    user_input.cursor = 0;
}

fn cursor_to_eol(user_input: &mut User_Input) {
    user_input.cursor = user_input.size;
}

fn word_forward(user_input: &mut User_Input) -> () {
    // special chars to ignore: ' ', '"', '\''
    let specs = [b' ', b'"', b'\''];
    while user_input.cursor < user_input.size && specs.contains(&user_input.bytes[user_input.cursor]) {
        user_input.cursor += 1;
    }
    while user_input.cursor < user_input.size && !specs.contains(&user_input.bytes[user_input.cursor]) {
        user_input.cursor += 1;
    }
}

fn word_backward(user_input: &mut User_Input) -> () {
    // if cursor at end of string, force one backward
    if user_input.cursor > 0 && user_input.cursor == user_input.size {
        user_input.cursor -= 1;
    }
    
    // special chars to ignore: ' ', '"', '\''
    let specs = [b' ', b'"', b'\''];
    while user_input.cursor > 0 && specs.contains(&user_input.bytes[user_input.cursor]) {
        user_input.cursor -= 1;
    }
    while user_input.cursor > 0 && !specs.contains(&user_input.bytes[user_input.cursor]) {
        user_input.cursor -= 1;
    }
}

fn delete_word_backward(user_input: &mut User_Input) -> () {
    // if cursor at end of string, force one backward
    //if user_input.size > 0 && user_input.cursor == user_input.size {
    //    user_input.cursor -= 1;
    //}
    let specs = [b' ', b'"', b'\''];
    
    while user_input.size > 0 && user_input.cursor > 0 && specs.contains(&user_input.bytes[user_input.cursor-1])  {
        for i in user_input.cursor-1..user_input.size-1 {
            user_input.bytes[i] = user_input.bytes[i+1];
        }
        user_input.cursor -= 1;
        user_input.size -= 1;
        user_input.bytes[user_input.size] = b'\0';
    }

    // specs stopping going further back
    while user_input.size > 0 && user_input.cursor > 0 && !specs.contains(&user_input.bytes[user_input.cursor-1]) {
        for i in user_input.cursor-1..user_input.size-1 {
            user_input.bytes[i] = user_input.bytes[i+1];
        }
        user_input.cursor -= 1;
        user_input.size -= 1;
        user_input.bytes[user_input.size] = b'\0';
    }

}

fn print_user_input(input: &mut User_Input) -> () {

    /* print a prompt if you want */
    let prompt_color = 3;
    print!("\x1b[1;{}mrush> \x1b[0m", prompt_color);
    ////////////////////////////////

    let cursor_color = 7; // white
    
    for i in 0..input.size {
        if i == input.cursor {
            print!("\x1B[{}m", cursor_color);
        }
        print!("{}",input.bytes[i] as char);
        print!("\x1B[0m");
    }
    if input.cursor == input.size {
        // emulate block cursor with space lol
        print!("\x1B[{}m \x1B[0m", cursor_color);
    }
    
}

fn grab_matching_cmds<'a>(history_cmds: &mut HashMap<&'a str,usize>, history: &'a String, input: &mut [u8;512]) -> usize {
    let cmds = history.split("\n");
    let search_string = str::from_utf8(input)
        .unwrap()
        .trim_end_matches('\0');

    history_cmds.clear();

    let mut display_count = 0;
    let mut cmd_score = 0; // will be changed later
    
    for cmd in cmds {
        if cmd.contains(search_string) {
            // save all of them to store in a hashmap
            history_cmds.insert(cmd, cmd_score);
            cmd_score += 1;
            if display_count < MAX_ROWS {
                display_count += 1;
            }
        }
    }

    return display_count;
}

fn display_cmds(history_cmds: &HashMap<&str,usize>, start_index: usize, highlight_cursor: usize) {
    // start_index: which cmd index to start displaying cmds from
    /* display cmds */
    let mut cmd_match_count: usize = 0;
    
    for (i, (&cmd, &_)) in history_cmds.into_iter().enumerate() {
        if i < start_index { continue }
        if cmd_match_count < MAX_ROWS {
            if i == highlight_cursor {
                print!("\n    \x1b[0;37;7m{}\x1b[0m", cmd);
            } else {
                print!("\n    \x1b[0m{}", cmd);
            }
            cmd_match_count += 1;
        }
    }
}

fn main() -> std::io::Result<()> {
    
    ***REMOVED***
    let history_env_var = "HISTFILE";
    let history_file_path: &str = &env::var(history_env_var)
        .map_err(|error| print!("{error}: Could not find environment variable \"{}\"", history_env_var))
        .unwrap();
    
    let mut file: File = File::open(history_file_path)?;
    
    let mut history = String::new();
    let _size = file.read_to_string(&mut history);

    //println!("[x] Read history file \"{}\" (size = {:?})", history_file_path, size);
    
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
    
    let mut user_input = User_Input {
        bytes: [0;512],
        size: 0,
        cursor: 0,
    };

    // read user input in loop
    let mut stdin = io::stdin().lock();

    let mut escape_mode: bool = false;

    let mut display_cmd_count: usize = 0;
    let mut previously_displayed_cmd: usize = 0;
    
    let mut cmd_start_index: usize = 0;
    let mut cmd_highlight_cursor: usize = 0;

    // HashMap:  score => cmd
    let mut history_cmds: HashMap<&str,usize> = HashMap::new();

    /* immediately print user prompt */
    print_user_input(&mut user_input);
    // TODO: do we want to print cmds by default or only when user input?
    io::stdout().flush().unwrap();
    
    //read byte by byte
    loop {
        stdin.read_exact(&mut raw_key).unwrap();
        let key = raw_key[0];

        // TODO: which key was pressed?
        match key {
            
            CTRL_c => {
                //erase_current_output(cmd_match_count);
                break;
            }, // Ctrl+c
            
            ESC => {
                escape_mode = true;
                continue;
            },
            
            _ => {
                //print!("key = <{}> ", key);
                //io::stdout().flush().unwrap();
                
                previously_displayed_cmd = display_cmd_count;
                        
                if !escape_mode {
                    
                    if key == ENTER || key == CARRIAGE {
                        // TODO : grab focused cmd, insert in current shell, exit rush
                        //erase_current_output(previously_displayed_cmd);
                        //print!("\n[!] select a cmd is not implemented yet\n");
                        break;
                        
                    } else if key == CTRL_a {
                        cursor_to_bol(&mut user_input);
                        
                    } else if key == CTRL_e {
                        cursor_to_eol(&mut user_input);

                    } else if key == CTRL_f {
                        cursor_forward(&mut user_input);
                        
                    } else if key == CTRL_b {
                        cursor_backward(&mut user_input);
                        
                    } else if key == CTRL_n {
                        if history_cmds.len() != 0 {
                            cmd_highlight_cursor = (cmd_highlight_cursor + 1) % history_cmds.len();

                            if cmd_start_index + MAX_ROWS <= cmd_highlight_cursor {
                                cmd_start_index += 1;
                            }

                            if cmd_highlight_cursor < cmd_start_index {
                                cmd_start_index = cmd_highlight_cursor;
                            }
                        }
                        
                    } else if key == CTRL_p {
                        if history_cmds.len() != 0 {
                            cmd_highlight_cursor = ((cmd_highlight_cursor as isize - 1) + history_cmds.len() as isize) as usize % history_cmds.len();

                            if cmd_highlight_cursor >= cmd_start_index + MAX_ROWS {
                                cmd_start_index = ((history_cmds.len() as isize - MAX_ROWS as isize) as usize + history_cmds.len()) % history_cmds.len();
                            } else if cmd_highlight_cursor < cmd_start_index {
                                cmd_start_index = cmd_highlight_cursor;
                            }
                        }

                    } else if key == BACKSPACE {
                        delete_backward(&mut user_input);
                        /* and reset cmd_cursors */
                        cmd_highlight_cursor = 0;
                        cmd_start_index = 0;
                        
                    } else {
                        insert_key(&mut user_input, key);

                        /* and reset cmd_cursors */
                        cmd_highlight_cursor = 0;
                        cmd_start_index = 0;
                    }
                    
                } else {
                    
                    escape_mode = false;

                    if key == b'b' {
                        word_backward(&mut user_input);
                        
                    } else if key == b'f' {
                        word_forward(&mut user_input);
                        
                    } else if key == BACKSPACE {
                        delete_word_backward(&mut user_input);
                        /* and reset cmd_cursors */
                        cmd_highlight_cursor = 0;
                        cmd_start_index = 0;
                    }
                    
                }

                /* screen display */
                erase_current_output(previously_displayed_cmd);
                print_user_input(&mut user_input);
                display_cmd_count = grab_matching_cmds(&mut history_cmds, &history, &mut user_input.bytes);
                display_cmds(&history_cmds, cmd_start_index, cmd_highlight_cursor);

            },
            
        }
        io::stdout().flush().unwrap();
    }
    
    erase_current_output(previously_displayed_cmd);
    restore_terminal(&mut terminal_at_start);

    ////////////////////////////////////////////////////////////
    // trying to print in input buffer the selected command
    let mut selected = "";
    for (i, (cmd, _score)) in history_cmds.into_iter().enumerate() {
       if i == cmd_highlight_cursor {
           selected = &cmd;
           break;
       }
    }
    
    if selected.len() != 0 {
        // launch xdotool but fuck that dependency...
        Command::new("xdotool")
            .arg("type")
            .arg("--delay")
            .arg("0")
            .arg(selected)
            .spawn()
            .expect("`xdotool` is not installed");
    }
    //// werid because work on root but not me
    //unsafe {
    //    for b in selected.bytes() {
    //        let mut ch = b as i8;
    //        let res = ioctl(0,0x5412, &mut ch as *mut i8);
    //        print!("{}",b as char);
    //    }
    //
    //}

    //io::stdout().flush().unwrap();
    
    Ok(())
}
