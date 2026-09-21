#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

use std::ffi::{c_int,c_uint,c_uchar,c_ulong,c_ushort};
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::env;
use std::cmp::min;
use std::collections::HashMap;

type cc_t = c_uchar;
type speed_t = c_uint;
type tcflag_t = c_uint;

const NCCS: usize = 32;

#[derive(Debug,Default)] // Default allows 0-initialization, Debug for printing with {:?}
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

struct winsize {
	ws_row: c_ushort,
	ws_col: c_ushort,
	_ws_xpixel: c_ushort,
	_ws_ypixel: c_ushort,
}

unsafe extern "C" {
    fn tcgetattr(fd: c_int, termios_p: *mut Termios) -> c_int;
    fn tcsetattr(fd: c_int, optional_actions: c_int, termios_p: *const Termios) -> c_int;
    fn ioctl(fd: c_int, op: c_ulong, ...) -> c_int;
}

const MAX_CMD_LEN: usize = 512;

struct User_Input {
    bytes: [u8;MAX_CMD_LEN],
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
const CTRL_u: u8 = 21; // delete back from cursor to bol
const CTRL_k: u8 = 11; // delete forward from cursor to eol

/* settigns */
const MAX_ROWS: usize = 20; // display a maximum of 10 matching lines

/* windows size */
const TIOCGWINSZ: c_ulong = 0x5413;

//const prompt: &str = "\x1b[1;30mrush> \x1b[0m";
const prompt_text: &str = " > ";

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

fn delete_from_cursor_to_bol(user_input: &mut User_Input) {
    // NOTE: we need to null terminate '\0'

    ////////////////////////////////////////
    // tringsas|t|ring
    // ^
    if user_input.cursor > 0 {
        for i in user_input.cursor..user_input.size {
            user_input.bytes[i-user_input.cursor] = user_input.bytes[i];
        }
        user_input.size -= user_input.cursor;
        user_input.cursor = 0;
        for i in user_input.size..MAX_CMD_LEN {
            user_input.bytes[i] = b'\0';
        }
    }
}

fn delete_from_cursor_to_eol(user_input: &mut User_Input) {
    // NOTE: we need to null terminate '\0'
    if user_input.cursor < user_input.size {
        for i in user_input.cursor..user_input.size {
            user_input.bytes[i] = b'\0';
        }
        user_input.size -= user_input.size - user_input.cursor;
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
    
    ////////////////////////////////
    /* print a prompt if you want */
    let prompt: String = format!("\x1b[1;32;48;5;237m{}\x1b[0m", prompt_text);
    print!("{}",prompt);
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

fn set_cmds_scores<'a>(history_cmds: &mut HashMap<&'a str,(usize,usize)>, user_input: &mut [u8;MAX_CMD_LEN]) {
    
    let history_count = history_cmds.len();
    
    let search = str::from_utf8(user_input)
        .unwrap()
        .trim_end_matches('\0')
        .to_string();

    let terms = search.trim().split_whitespace();
    
    'outer: for (cmd,(index,score)) in history_cmds.iter_mut() {
        
        *score = 0;

        if cmd.is_empty() { continue }

        for term in terms.clone() {
            
            if cmd.to_lowercase().contains(&term.to_lowercase()) {
                *score += history_count;
            } else {
                *score = 0;
                continue 'outer;
            }
            
        }

        if *score > 0 || search.is_empty() { // when search empty keep all history lines
            *score += *index;
        }
    }
}

fn display_cmds<'a>(history_cmds: &'a HashMap<&'a str,(usize,usize)>, min_score: usize, start_index: usize, highlight_cursor: usize, _rows: u16, cols: u16) -> &'a str {

    /* display cmds */
    let mut displayed: usize = 0;
    let mut selected = "";
    
    let pad = prompt_text.len() + 10;
    let c = cols as usize - pad;

    // TODO: abstract this out of function and compute only when we need to rescore
    let mut filtered:Vec<_> = history_cmds.iter().filter(|(_,(_,score))| *score >= min_score).collect();
    filtered.sort_by(|a, b| b.1.cmp(a.1)); // descending sort scores

    let mut index = 0;

    // colors
    let default_color = "\x1b[0;37;49m";
    let highlight_color = "\x1b[1;37;48;5;237m";
    let default_headblock = "\x1b[0;32;48;5;237m";
    let highlight_headblock = "\x1b[0;;42m";

    for (cmd, _) in filtered {
        
        if index < start_index {
            index += 1;
            continue;
        }
        
        if displayed >= MAX_ROWS { break }

        print!("\n{}", " ".repeat(prompt_text.len()));
        if displayed == highlight_cursor {
            print!("{} \x1b[0m{}", highlight_headblock, highlight_color);
            selected = cmd;
        } else {
            print!("{} \x1b[0m{}", default_headblock, default_color);
        }
        //print!("{:.c$}\x1b[0m", cmd);
        print!(" {:c$}\x1b[0m", &cmd[..c.min(cmd.len())].replace('\t',"    ")); // tabs can mess with output  

        displayed += 1;
        index += 1;
    }

    return selected
}

fn update_term_size(rows: &mut u16, cols: &mut u16) {
    unsafe {
        let w: winsize = std::mem::zeroed();
        ioctl(STDIN,TIOCGWINSZ,&w);
        //println!("w.ws_row = {:?} ; w.ws_col = {:?}", w.ws_row, w.ws_col);
        *rows = w.ws_row as u16;
        *cols = w.ws_col as u16;
    }
}

fn main() -> std::io::Result<()> {

    let mut rows: u16 = 0;
    let mut cols: u16 = 0;
    update_term_size(&mut rows,&mut cols);
    
    ***REMOVED***
    let history_env_var = "HISTFILE";
    let history_file_path: &str = &env::var(history_env_var)
        .map_err(|error| print!("{error}: Could not find environment variable \"{}\"", history_env_var))
        .unwrap();
    
    let mut file: File = File::open(history_file_path)?;
    
    let mut history = String::new();

    match file.read_to_string(&mut history) {
        Err(e) => return Err(e), // could not read file
        Ok(_) => {}, // go on peacefully
    }

    // HashMap:  score => cmd
    let mut history_cmds: HashMap<&str,(usize,usize)> = HashMap::new();

    // populate history_cmds with scores = 0
    let cmds = history.split("\n");
    let mut index = 0;
    for cmd in cmds {
        if cmd.is_empty() { continue }
        if !history_cmds.contains_key(cmd) {
            index += 1;
            history_cmds.insert(cmd,(index,0));
        }
    }
    
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
        bytes: [0;MAX_CMD_LEN],
        size: 0,
        cursor: 0,
    };

    let mut escape_mode: bool = false;
    
    let mut cmd_start_index: usize = 0;
    let mut cmd_highlight_cursor: usize = 0;

    /* immediately print user prompt  and compute first iteration */
    print_user_input(&mut user_input);
    set_cmds_scores(&mut history_cmds, &mut user_input.bytes);
    let mut selected = display_cmds(&history_cmds,0, 0, 0, rows, cols);
    io::stdout().flush().unwrap();    

    let mut command_was_selected: bool = false;

    let mut available_cmds;
    available_cmds = history_cmds.len();

    let mut displayed_count: usize = min(MAX_ROWS,available_cmds);
    
    // read user input in loop
    let mut stdin = io::stdin().lock();

    let mut rescore: bool;
    
    //read byte by byte
    loop {
        
        stdin.read_exact(&mut raw_key).unwrap();
        let key = raw_key[0];

        match key {
            
            ESC => {
                escape_mode = true;
                continue;
            },
            
            _ => {
                //print!("key = <{}> \n", key);
                //io::stdout().flush().unwrap();

                rescore = true;
                
                if user_input.bytes[0] == 0 {
                    available_cmds = history_cmds.len();
                }
                
                if key == CTRL_c {
                    break;
                }
                
                if !escape_mode {
                    
                    if key == ENTER || key == CARRIAGE {
                        command_was_selected = true;
                        break;
                        
                    } else if key == CTRL_a {
                        cursor_to_bol(&mut user_input);
                        
                    } else if key == CTRL_e {
                        cursor_to_eol(&mut user_input);

                    } else if key == CTRL_f {
                        cursor_forward(&mut user_input);
                        rescore = false;
                        
                    } else if key == CTRL_b {
                        cursor_backward(&mut user_input);
                        rescore = false;
                        
                    } else if key == CTRL_k {
                        delete_from_cursor_to_eol(&mut user_input);
                        cmd_highlight_cursor = 0;
                        cmd_start_index = 0;
                        
                    } else if key == CTRL_u {
                        delete_from_cursor_to_bol(&mut user_input);
                        cmd_highlight_cursor = 0;
                        cmd_start_index = 0;
                        
                    } else if key == CTRL_n {
                        if available_cmds != 0 {

                            cmd_highlight_cursor += 1;
                            
                            if cmd_highlight_cursor >= MAX_ROWS {
                                cmd_start_index += 1;
                            } else {
                                rescore = false;
                            }

                            if available_cmds > MAX_ROWS && cmd_start_index + MAX_ROWS >= available_cmds {
                                cmd_start_index = 0;
                                cmd_highlight_cursor = 0;
                            }

                            if cmd_highlight_cursor >= available_cmds {
                                cmd_start_index = 0;
                                cmd_highlight_cursor = 0;
                            }

                            cmd_highlight_cursor = if cmd_highlight_cursor >= MAX_ROWS { MAX_ROWS - 1 } else { cmd_highlight_cursor };
                            cmd_highlight_cursor = if cmd_highlight_cursor >= available_cmds { available_cmds - 1 } else { cmd_highlight_cursor };

                        }
                        
                    } else if key == CTRL_p {
                        if available_cmds != 0 {
                            
                            if cmd_highlight_cursor == 0 {
                                if cmd_start_index == 0 {
                                    if available_cmds <= MAX_ROWS {
                                        rescore = false;
                                    }
                                    cmd_highlight_cursor = if available_cmds > MAX_ROWS {MAX_ROWS - 1} else {available_cmds - 1};
                                    cmd_start_index      = if available_cmds > MAX_ROWS {available_cmds - MAX_ROWS - 1} else {0};
                                } else {
                                    cmd_start_index -= 1;
                                }
                            } else {
                                cmd_highlight_cursor -= 1;
                                rescore = false;
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
                        rescore = false;
                        
                    } else if key == b'f' {
                        word_forward(&mut user_input);
                        rescore = false;
                        
                    } else if key == BACKSPACE {
                        delete_word_backward(&mut user_input);
                        /* and reset cmd_cursors */
                        cmd_highlight_cursor = 0;
                        cmd_start_index = 0;
                    }
                    
                }

                /* compute scores */
                if rescore {
                    set_cmds_scores(&mut history_cmds, &mut user_input.bytes);
                    available_cmds = history_cmds.values().filter(|(_,score)| *score > 0).count();
                } 

                /* screen display */
                update_term_size(&mut rows,&mut cols);
                erase_current_output(displayed_count);
                print_user_input(&mut user_input);
                selected = display_cmds(&history_cmds,1, cmd_start_index, cmd_highlight_cursor, rows, cols);

                /* recompute displayed */
                displayed_count = if available_cmds > MAX_ROWS {MAX_ROWS} else {available_cmds};// - cmd_start_index;
            },

        }
        io::stdout().flush().unwrap();
    }
    
    erase_current_output(displayed_count);
    restore_terminal(&mut terminal_at_start);

    ////////////////////////////////////////////////////////////
    // trying to print in input buffer the selected command

    print!("\r\x1B[1A\r");
    
    if command_was_selected {
        print!("\n{}", if selected.len() > 0 { selected } else {" "});
    } else {
        print!("\n ");
    }
    
    Ok(())
}
