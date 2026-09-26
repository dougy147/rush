#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

use std::io;
use std::env;
use std::thread;
use std::cmp::min;
use std::path::Path;
use std::io::{Read, Write};
use std::fs::{File,OpenOptions};
use std::process::{Command,Stdio};
use std::collections::{BTreeMap,HashMap};
use std::os::fd::AsRawFd;
use std::sync::mpsc::channel;

pub const MAX_SEARCH_LEN: usize = 512;
pub const PROMPT_TEXT: &str = " > ";

/* modules */
mod search; use search::*;
mod modes; use modes::*;
mod term; use term::*;

/* modes and submodes */
#[derive(PartialEq)]
enum Mode {
    NONE,
    HISTORY, //  => default mode?
    FILE,    //  => display file in rush (same as HISTORY but reversed display)
    COMMAND, //  => execute a command and navigate through output
    COMPILE, //  => same as above but no user_search filtering (no prompt)
    STDIN,   //  when piping something to rush
}

#[derive(PartialEq)]
enum Submode {
    NONE,
    INSERT, // insert selection in command line input (default in HISTORY mode)
    PRINT,  // echo selection to STDOUT
    OPEN,   // open line in EDITOR
}

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
const CTRL_l: u8 = 12; // take the whole screen
const CTRL_r: u8 = 18; // equivalent to CTRL_c in HISTORY mode?

fn parse_args(prog_name: &mut String, mode: &mut Mode, submode: &mut Submode, content_file_path: &mut String, cmd_string: &mut String) {
    let args: Vec<String> = env::args().collect();
    *prog_name = args[0].clone();
    
    for (i, arg) in args.iter().skip(1).enumerate() {
        let a: &str = arg;

        if a.chars().nth(0).unwrap() == '-' {
            // parse flag
            if a.chars().count() == 1 {
                *mode = Mode::STDIN;
            }

            for c in a.chars().skip(1) {
                match c {
                    /* modes */
                    'H' => {
                        *mode = Mode::HISTORY;
                        if *submode == Submode::NONE { *submode = Submode::INSERT }
                    },
                    'F' => {
                        *mode = Mode::FILE;
                        if *submode == Submode::NONE { *submode = Submode::OPEN }
                    },
                    'X' => {
                        *mode = Mode::COMMAND;
                        if *submode == Submode::NONE { *submode = Submode::PRINT }
                    },
                    'C' => {
                        *mode = Mode::COMPILE;
                        *submode = Submode::OPEN;
                    },
                    
                    /* submodes */
                    'i' => { // NOTE: inserting in terminal needs a wrapper script
                             //       hence `rush.bash`. TODO: for other shells
                        *submode = Submode::INSERT;
                    },
                    'p' => {
                        *submode = Submode::PRINT;
                    },
                    'o' => {
                        *submode = Submode::OPEN;
                    },

                    _ => {
                        println!("Ignoring unknown flag: {}", c);
                        // TODO: usage(1);
                    }
                }
            }

        } else {

            if *mode == Mode::FILE || *mode == Mode::HISTORY {
                *content_file_path = a.to_string();
                
            } else if *mode == Mode::COMMAND || *mode == Mode::COMPILE {
                // grab remaining as cmd single string and break
                for elem in &args[i+1..] {
                    cmd_string.push(' ');
                    cmd_string.push_str(elem);
                }
                break;
            }
        }
    }
}

#[allow(nonstandard_style)]
fn main() -> std::io::Result<()> {

    /* prepare terminal */
    let mut original_terminal = Rush_Term::new();
    original_terminal.save();
    
    let mut terminal = Rush_Term::new();
    terminal.save();
    terminal.enable_raw_mode();
    terminal.update_term_size();
    
    let mut MAX_ROWS: usize = min(20,terminal.height as usize - 1);
    assert!(MAX_ROWS > 1);
    
    /* prepare modes and content */
    let mut mode = Mode::NONE;
    let mut submode = Submode::NONE;
    let mut content_file_path = Default::default();
    let mut cmd_string = Default::default(); // in case of COMPILE mode
    
    /* initialize arguments */
    let mut prog_name = Default::default();
    parse_args(&mut prog_name, &mut mode, &mut submode, &mut content_file_path, &mut cmd_string);
    
    if mode == Mode::NONE {
        println!("{} no mode provided. TODO USAGE", prog_name);
        original_terminal.restore();
        return Ok(());
    }

    assert!(submode != Submode::NONE);

    /* populate content */
    let mut content_map: BTreeMap<(usize, &str),usize> = BTreeMap::new();
    let mut visited: HashMap<&str,u8> = HashMap::new(); // used in STDIN mode
    let mut content_bytes = Vec::new();
    let content;

    /* HISTORY and FILE modes both read files */
    if mode == Mode::HISTORY || mode == Mode::FILE {

        // HISTORY : If no file provided, try reading HISTFILE environment variable
        if mode == Mode::HISTORY && content_file_path.is_empty() {
            let content_env_var = "HISTFILE";
            content_file_path = match env::var(content_env_var) {
                Err(e) => {
                    print!("{e}: \"{}\"", content_env_var);
                    original_terminal.restore();
                    return Ok(())
                },
                Ok(content) => content,
            };
        }
       
        match File::open(content_file_path.clone()) {
            Err(e) => {
                original_terminal.restore();
                return Err(e) // could not open file
            }
            Ok(mut file)  => {
                match file.read_to_end(&mut content_bytes) {
                    Err(e) => {
                        original_terminal.restore();
                        return Err(e) // could not read content
                    },
                    Ok(_) => {
                        content = String::from_utf8_lossy(&content_bytes).to_string();
                    }, // go on peacefully
                }
            },
        };

        if mode == Mode::HISTORY {

            let count = content.trim_end().split("\n").count();

            let mut index = 0;
            for line in content.trim_end().split("\n") {

                // if you want duplicates, comment this block
                {
                    if line.is_empty() { continue }
                    if visited.contains_key(line) { continue }
                    visited.insert(line,0);
                }

                content_map.insert((count - index,line),count - index);
                index += 1;
            }
        }

        if mode == Mode::FILE {
            let mut index = 0;
            for line in content.trim_end().split("\n") {
                index += 1;
                content_map.insert((index,line),index);
            }
        }
        
    } else if mode == Mode::COMPILE || mode == Mode::COMMAND {
        // TODO: Ctrl+c should stop capturing!!!
        
        let mut output: Vec<(String,usize)> = Vec::new();
        cmd_capture_output(&cmd_string, &mut output);
        
        let mut index = 0;
        for (line,stream) in output {
            index += 1;
            let boxed = Box::leak(line.into_boxed_str()); // this is necessary as long as lines are stored as &str instead of String
            content_map.insert((index,boxed),stream as usize);
        }
        
    } else if mode == Mode::STDIN {
        
        match io::stdin().read_to_end(&mut content_bytes) {
            Err(e) => return Err(e),
            Ok(_)  => {
                content = String::from_utf8_lossy(&content_bytes).to_string();
            },
        }

        let mut index = 0;
        for line in content.trim_end().split("\n") {
            index += 1;
            content_map.insert((index,line),index);
        }

    }

    /* uncomment this block if COMMAND or COMPILE output was echoed
       in stdout and you want to override it */
    //if mode == Mode::COMPILE || mode == Mode::COMMAND {
    //    rush_term.erase_up(content_map.len());
    //    io::stdout().flush().unwrap();
    //}

    /* prepare user search input */
    let mut raw_key = [0u8;1];
    let mut user_search = User_Search::new();

    let mut escape_mode: bool = false;

    /* prepare display */
    let mut line_start_index: usize = 0;
    let mut line_highlight_cursor: usize = 0;

    let mut command_was_selected: bool = false;
    let mut showable_lines = content_map.len();
    let mut displayed_count: usize = min(MAX_ROWS,showable_lines);

    let mut rescore: bool; // sometimes there is no need to recompute scores
    
    /* read user search from stdin in loop */
    // https://users.rust-lang.org/t/how-to-read-user-input-again-in-pipeline/50576    
    // We cannot use `stdin` for both reading content AND user key press, so we open a new one here
    let mut stdin = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
    let fd = stdin.as_raw_fd();
    
    if mode == Mode::STDIN {
        original_terminal.fd = fd;
        original_terminal.save();
        terminal.fd = fd;
        terminal.save();
        terminal.enable_raw_mode();
    }
    
    if content_map.len() == 0 {
        // no content to display
        original_terminal.restore();
        return Ok(());
    }
    
    /* immediately print user prompt  and compute first iteration except in COMPILE mode */
    if mode != Mode::COMPILE {
        user_search.print();
        let _ = compute_scores(&mut content_map, &mut user_search.bytes, &mode);
    }

    let mut selected;
    if mode == Mode::COMPILE {
        selected = display_lines_compile(&content_map,line_start_index, &mut line_highlight_cursor, MAX_ROWS, &terminal);
    } else {
        selected = display_lines_default(&content_map,0, 0, 0, MAX_ROWS, &terminal, &mode);
    }
    io::stdout().flush().unwrap();    
    
    /* let's gooo */
    loop {

        stdin.read_exact(&mut raw_key).unwrap();
        let key = raw_key[0];
        //print!("key = <{}> \n\n\n", key);

        rescore = false;
        
        match key {

            ESC => {
                escape_mode = true;
                continue;
            },
            
            _ => {
                
                if user_search.size == 0 {
                    showable_lines = content_map.len();
                }
                
                if key == CTRL_c {
                    break;
                }

                if key == CTRL_r && mode == Mode::HISTORY {
                    break;
                }
                
                if !escape_mode {
                    
                    if key == ENTER || key == CARRIAGE {

                        if submode == Submode::OPEN {
                            open_in_editor(selected,content_file_path.clone(),&mode);
                            terminal.hide_cursor();
                        } else {
                            command_was_selected = true;
                            break;
                        }
                        
                    } else if key == CTRL_a {
                        user_search.cursor_to_bol();
                        
                    } else if key == CTRL_e {
                        user_search.cursor_to_eol();

                    } else if key == CTRL_f {
                        user_search.cursor_forward();
                        rescore = false;
                        
                    } else if key == CTRL_b {
                        user_search.cursor_backward();
                        rescore = false;
                        
                    } else if key == CTRL_k {
                        user_search.delete_from_cursor_to_eol();
                        line_highlight_cursor = 0;
                        line_start_index = 0;
                        rescore = true;

                    } else if key == CTRL_l {
                        terminal.clear();
                        terminal.update_term_size();
                        MAX_ROWS = terminal.height as usize - 1;
                        line_highlight_cursor = 0;
                        line_start_index = 0;
                        
                    } else if key == CTRL_u {
                        user_search.delete_from_cursor_to_bol();
                        line_highlight_cursor = 0;
                        line_start_index = 0;
                        rescore = true;
                        
                    } else if key == CTRL_n {
                        if showable_lines != 0 {

                            line_highlight_cursor += 1;
                            
                            if line_highlight_cursor > MAX_ROWS - 1 {
                                line_start_index += 1;
                            }

                            if showable_lines > MAX_ROWS && line_start_index + MAX_ROWS > showable_lines {
                                line_start_index = 0;
                                line_highlight_cursor = 0;
                            }

                            if line_highlight_cursor >= showable_lines {
                                line_start_index = 0;
                                line_highlight_cursor = 0;
                            }

                            line_highlight_cursor = if line_highlight_cursor >= MAX_ROWS { MAX_ROWS - 1 } else { line_highlight_cursor };
                            line_highlight_cursor = if line_highlight_cursor >= showable_lines { showable_lines - 1 } else { line_highlight_cursor };

                        }
                        
                    } else if key == CTRL_p {
                        if showable_lines != 0 {
                            
                            if line_highlight_cursor == 0 {
                                if line_start_index == 0 {
                                    line_highlight_cursor = if showable_lines > MAX_ROWS {MAX_ROWS - 1} else {showable_lines - 1};
                                    line_start_index      = if showable_lines > MAX_ROWS {showable_lines - MAX_ROWS} else {0};
                                } else {
                                    line_start_index -= 1;
                                }
                            } else {
                                line_highlight_cursor -= 1;
                            }

                        }

                    } else if key == BACKSPACE {
                        user_search.delete_backward();
                        /* and reset line_cursors */
                        line_highlight_cursor = 0;
                        line_start_index = 0;
                        rescore = true;
                        
                    } else {

                        user_search.insert_key(key);

                        if key == b'g' && mode == Mode::COMPILE {
                            /* poor attempt at mimicking emacs compilation mode */
                            let mut output: Vec<(String,usize)> = Vec::new();
                            cmd_capture_output(&cmd_string, &mut output);

                            content_map.clear();

                            let mut index = 0;
                            for (line,stream) in output {
                                index += 1;
                                let boxed = Box::leak(line.into_boxed_str());
                                content_map.insert((index,boxed),stream as usize);
                            }

                            line_highlight_cursor = 0;
                            line_start_index = 0;

                            let _ = compute_scores(&mut content_map, &mut user_search.bytes, &mode);
                            showable_lines = content_map.len();
                            
                        } else if key != b' ' || user_search.cursor < user_search.size {
                            /* if not a space rescore */
                            /* and reset line_cursors */
                            rescore = true;
                            line_highlight_cursor = 0;
                            line_start_index = 0;
                        }
                    }
                    
                } else {
                    
                    escape_mode = false;

                    if key == b'b' {
                        user_search.word_backward();
                        rescore = false;
                        
                    } else if key == b'f' {
                        user_search.word_forward();
                        rescore = false;
                        
                    } else if key == BACKSPACE {
                        user_search.delete_word_backward();
                        /* and reset line_cursors */
                        line_highlight_cursor = 0;
                        line_start_index = 0;
                        rescore = true;
                    }
                    
                }

                /* compute scores */
                if rescore && mode != Mode::COMPILE {                    
                    let _ = compute_scores(&mut content_map, &mut user_search.bytes, &mode);
                    showable_lines = content_map.values().filter(|score| **score > 0).count();
                }

                /* screen display */
                terminal.update_term_size();
                terminal.erase_up(displayed_count);

                if mode != Mode::COMPILE {
                    user_search.print();
                    selected = display_lines_default(&content_map,1, line_start_index, line_highlight_cursor, MAX_ROWS, &terminal, &mode);
                } else {
                    selected = display_lines_compile(&content_map,line_start_index, &mut line_highlight_cursor, MAX_ROWS, &terminal);
                }
                
                /* recompute displayed */
                displayed_count = if showable_lines > MAX_ROWS {MAX_ROWS} else {showable_lines};// - line_start_index;
            },

        }
        io::stdout().flush().unwrap();
    }
    
    terminal.erase_up(displayed_count);
    original_terminal.restore();

    print!("\r\x1B[1A\r"); // clean current line
    
    if command_was_selected {
        if submode == Submode::INSERT {
            print!("\n{}", if selected.1.len() > 0 { selected.1 } else {" "});
        } else if submode == Submode::PRINT {
            print!("\n{}", if selected.1.len() > 0 { selected.1 } else {" "});
        } else if submode == Submode::OPEN {
            open_in_editor(selected,content_file_path,&mode);            
        }
    } else {
        print!("\n ");
    }
    
    Ok(())
}
