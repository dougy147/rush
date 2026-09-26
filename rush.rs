#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

use std::io;
use std::env;
use std::thread;

use std::cmp::min;
use std::path::Path;
use std::io::{BufRead, BufReader, Read, Write};
use std::fs::{File,OpenOptions};
use std::process::{Command,Stdio};
use std::collections::{BTreeMap,HashMap};

use std::os::fd::AsRawFd;
use std::sync::mpsc::channel;

pub const MAX_SEARCH_LEN: usize = 512;

// our modules
mod user_search;
mod term;

use user_search::*;
use term::*;

struct Location {
    file_path: String,
    row: u64,
    col: u64,
}

impl Location {
    fn new() -> Self {
        Self {
            file_path: "".to_string(),
            row: 0,
            col: 0,
        }
    }
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

const prompt_text: &str = " > ";

/* Colors */
const default_color       : &str = "\x1b[0;37;49m";
const highlight_color     : &str = "\x1b[1;37;48;5;237m";
const default_headblock   : &str = "\x1b[0;32;48;5;237m";
const highlight_headblock : &str = "\x1b[0;;42m";

// for COMPILE colors
const default_err_color       : &str = "\x1b[0;31;48;5;235m";
const highlight_err_color     : &str = "\x1b[1;31;48;5;237m";
const default_err_headblock   : &str = "\x1b[0;32;48;5;1m";
const highlight_err_headblock : &str = "\x1b[0;;41m";

const default_empty_line_color       : &str = "\x1b[0;30;48;5;235m";
const highlight_empty_line_color     : &str = "\x1b[1;30;48;5;237m";
const default_empty_line_headblock   : &str = "\x1b[1;32;48;5;241m";
const highlight_empty_line_headblock : &str = "\x1b[1;32;48;5;241m";

/* modes and submodes */
#[derive(PartialEq)]
enum Mode {
    NONE,
    HISTORY, // ALT_1  => default mode?
    FILE,    // ALT_2  => display file in rush (same as HISTORY but reversed display)
    COMMAND, // ALT_3  => execute a command and navigate through output
    COMPILE, // ALT_4  => same as above but no user_search filtering (no prompt)
    STDIN,   // when piping something to rush
}

#[derive(PartialEq)]
enum Submode {
    NONE,
    INSERT, // insert selection in command line input (default in HISTORY mode)
    PRINT,  // echo selection to STDOUT
    OPEN,   // open line in EDITOR
}

fn erase_current_output(rows: usize) -> () {
    if rows != 0 {
        print!("\x1B[{}K", rows); // erase matching cmd rows + user input
        print!("\x1B[{}A\x1B[0J", rows); // move cursor up and clean up lines
    }
    print!("\x1B[{}G", 0); // cursor to bol
    print!("\x1B[0K\r");
}

fn print_user_search(search: &mut User_Search) -> () {
    
    ////////////////////////////////
    /* print a prompt if you want */
    let prompt: String = format!("\x1b[1;32;48;5;237m{}\x1b[0m", prompt_text);
    print!("{}",prompt);
    ////////////////////////////////

    let cursor_color = 7; // white

    let text = String::from_utf8_lossy(&search.bytes[..search.size]);

    for (i,c) in text.chars().enumerate() {
        if i == search.cursor {
            print!("\x1B[{}m", cursor_color);
        }
        print!("{}",c);
        print!("\x1B[0m");
    }
    
    if search.cursor == search.size {
        // emulate block cursor with space lol
        print!("\x1B[{}m \x1B[0m", cursor_color);
    }
}

fn compute_scores<'a>(content: &mut BTreeMap<(usize, &'a str),usize>, user_search: &mut [u8;MAX_SEARCH_LEN], _mode: &Mode)  -> Result<(), Box<dyn std::error::Error>> {
    
    let content_count = content.len();
    
    let search = str::from_utf8(user_search)?
        .trim_end_matches('\0')
        .to_owned();

    let terms = search.trim().split_whitespace();
    
    'outer: for ((index,cmd),score) in content.iter_mut() {
        
        *score = 0;

        //if cmd.is_empty() { continue }

        for term in terms.clone() {
            
            if cmd.to_lowercase().contains(&term.to_lowercase()) {
                *score += content_count;
            } else {
                *score = 0;
                continue 'outer;
            }
            
        }

        if *score > 0 || search.is_empty() { // when search empty keep all content lines
            *score += *index;
        }
    }

    Ok(())
}

fn get_location_from_line(line: &str) -> Option<Location> {
    let mut loc: Location = Location::new();

    if !line.contains(":") { return None }

    for s in line.split_whitespace() {
        if s.contains(":") {
            let ss: Vec<&str> = s.split(":").collect();

            for (i,t) in ss.iter().enumerate() {
                let n0 = ss.get(i+1); // row?
                let n1 = ss.get(i+2); // col?
                if Path::new(t).is_file() {
                    loc.file_path = t.to_string();
                    if let Some(val0) = n0 {
                        if let Ok(row) = val0.parse::<u64>() {
                            loc.row = row;
                        }
                        if loc.row <= 0 { break }
                        if let Some(val1) = n1 {
                            if let Ok(col) = val1.parse::<u64>() {
                                loc.col = col;
                            }
                        }
                        return Some(loc)
                    }
                }
            }
        }
    }
    return None
}

fn display_lines_compile<'a>(content: &'a BTreeMap<(usize, &'a str),usize>, start_index: usize, highlight_cursor: &mut usize, max_rows: usize, term: &Rush_Term) -> (usize, &'a str) {

    //assert!(*mode == Mode::COMPILE);
    
    /* display cmds */
    let mut displayed: usize = 0;
    let mut selected: (usize, &'a str) = (0,""); // selected key from content_cmds
    
    let pad = prompt_text.len() + 10;
    let c = term.width as usize - pad;

    let mut index = 0;

    for (key,stream) in content.iter() {

        if index < start_index {
            index += 1;
            continue;
        }

        let (_, line) = *key;

        // grab location if any
        let location = get_location_from_line(line);
        match location {
            Some(_loc) => {
                //println!("{}>>{}>>{}", loc.file_path, loc.row, loc.col);
            },
            None => {},
        }

        if displayed >= max_rows { break }

        print!("\n{}", " ".repeat(prompt_text.len()));
        
        if displayed == *highlight_cursor as usize {

            if line.is_empty() {
                print!("{} \x1b[0m{}", highlight_empty_line_headblock, highlight_empty_line_color);
            } else if *stream == 2 {
                print!("{} \x1b[0m{}", highlight_err_headblock, highlight_err_color);
            } else {
                print!("{} \x1b[0m{}", highlight_headblock, highlight_color);
            }
            selected = *key;
            
        } else {

            if line.is_empty() {
                print!("{} \x1b[0m{}", default_empty_line_headblock, default_empty_line_color);
            } else if *stream == 2 {
                print!("{} \x1b[0m{}", default_err_headblock, default_err_color);
            } else {
                print!("{} \x1b[0m{}", default_headblock, default_color);
            }
            
        }

        print!(" {:c$}\x1b[0m",
               &line[..c.min(line.len())]
               .replace('\t',"    ")
               .replace('\n',"\\n")); // tabs and newline chars can mess with output
        
        displayed += 1;
        index += 1;
    }

    return selected
}
    
fn display_lines<'a>(content: &'a BTreeMap<(usize, &'a str),usize>, min_score: usize, start_index: usize, highlight_cursor: usize, max_rows: usize, term: &Rush_Term, mode: &Mode) -> (usize, &'a str) {
    
    assert!(*mode != Mode::COMPILE);
    
    /* display cmds */
    let mut displayed: usize = 0;
    let mut selected: (usize, &'a str) = (0,""); // selected key from content_cmds
    
    let pad = prompt_text.len() + 10;
    let c = term.width as usize - pad;

    let mut index = 0;

    for (key,_score) in content.iter().filter(|((_,_),s)| **s >= min_score) {
        
        if index < start_index {
            index += 1;
            continue;
        }

        let (_, cmd) = *key;
        
        if displayed >= max_rows { break }

        print!("\n{}", " ".repeat(prompt_text.len()));
        if displayed == highlight_cursor {
            print!("{} \x1b[0m{}", highlight_headblock, highlight_color);
            selected = *key;
        } else {
            print!("{} \x1b[0m{}", default_headblock, default_color);
        }
        //print!("{:.c$}\x1b[0m", cmd);
        print!(" {:c$}\x1b[0m",
               &cmd[..c.min(cmd.len())]
               .replace('\t',"    ")
               .replace('\n',"\\n")); // tabs and newline chars can mess with output  
        displayed += 1;
        index += 1;
    }

    return selected
}

fn cmd_capture_output<'a>(cmd_string: &String, content: &mut Vec<(String, usize)>) {
    // TODO: Ctrl+c should stop capturing!!!

    let mut cmd = Command::new("sh")
        .arg("-c")
        .arg(cmd_string)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let (tx, rx) = channel();
    
    let stdout = cmd.stdout.take().unwrap();
    let stderr = cmd.stderr.take().unwrap();

    let tx_err = tx.clone();
    thread::spawn(move || {
        BufReader::new(stderr).lines().for_each(|line| {
            let _ = tx_err.send((line.unwrap(), 2));
        });
    });

    thread::spawn(move || {
        BufReader::new(stdout).lines().for_each(|line| {
            let _ = tx.send((line.unwrap(),1));
        });
    });

    for (line, stream) in rx {
        // stream = 0 if stdout
        // stream = 2 if stderr
        let boxed = Box::leak(line.into_boxed_str()); // this is necessary as long as lines are stored as &str instead of String
        content.push((boxed.to_string(),stream));
        //println!("{}: {}", stream, boxed);
    }

    cmd.wait().unwrap();
}

fn open_in_editor(selected_line: (usize,&str), content_file_path: String, mode: &Mode) {
    let editor_env_var = "EDITOR";
    let editor = env::var(editor_env_var)
        .map_err(|error| print!("{error}: Could not find environment variable \"{}\"", editor_env_var))
        .unwrap();

    let file_path: String;
    let line_num;
    
    if *mode == Mode::COMPILE || *mode == Mode::COMMAND {
        match get_location_from_line(selected_line.1) {
            Some(loc) => {
                file_path = loc.file_path;
                line_num = loc.row;
            },
            None => return,
        }
    } else {
        file_path = content_file_path;
        line_num = selected_line.0 as u64;
    }

    // WARNING: only works with vim!!
    // TODO: make this compatible with other EDITORs
    Command::new(editor)
        .arg(file_path)
        .arg(format!("+{}",line_num))
        .status()
        .expect("`{editor}` should be executable");
}

fn parse_args(prog_name: &mut String, mode: &mut Mode, submode: &mut Submode, content_file_path: &mut String, cmd_string: &mut String) {
    let args: Vec<String> = env::args().collect();
    *prog_name = args[0].clone();
    
    for (i, arg) in args.iter().skip(1).enumerate() {
        let a: &str = arg;
        // TODO: parse flags to allow `rush -Fi` instead of `-F -i`
        
        match a {
            /* modeps */
            "--history" | "-H" => {
                *mode = Mode::HISTORY;
                if *submode == Submode::NONE { *submode = Submode::INSERT }
            },
            "--file"    | "-F" => {
                *mode = Mode::FILE;
                if *submode == Submode::NONE { *submode = Submode::OPEN }
            },
            "--command" | "-X" => {
                *mode = Mode::COMMAND;
                if *submode == Submode::NONE { *submode = Submode::PRINT }
            },
            "--compile" | "-C" => {
                *mode = Mode::COMPILE;
                *submode = Submode::OPEN;
            },
            "--stdin"   | "-"  => {
                *mode = Mode::STDIN;
            }
            
            /* submodes */
            "--insert" | "-i" => {
                // TODO: this needs a wrapper script to work
                //       (not implemented yet except for HISTORY mode)
                *submode = Submode::INSERT;
            },
            "--print" | "-p" => {
                *submode = Submode::PRINT;
            },
            "--open" | "-o" => {
                *submode = Submode::OPEN;
            },

            /* remaining */
            _ => {
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
            },
        }
    }
}

#[allow(nonstandard_style)]
fn main() -> std::io::Result<()> {
    //println!("term:  cols = {}", cols);

    let mut original_terminal = Rush_Term::new();
    original_terminal.save();
    //save_terminal(STDIN, &mut terminal_at_start);
    
    let mut rush_terminal = Rush_Term::new();
    rush_terminal.save();
    rush_terminal.enable_raw_mode();
    rush_terminal.update_term_size();
    
    let mut MAX_ROWS: usize = min(20,rush_terminal.height as usize - 1);
    assert!(MAX_ROWS > 1);
    
    /* mode */
    let mut mode = Mode::NONE;
    let mut submode = Submode::NONE;
    let mut content_file_path = Default::default();
    let mut cmd_string = Default::default(); // in case of COMPILE mode
    
    /* grab arguments */
    // mode, submode, content_file_path, cmd_string
    let mut prog_name = Default::default();
    parse_args(&mut prog_name, &mut mode, &mut submode, &mut content_file_path, &mut cmd_string);
    
    if mode == Mode::NONE {
        println!("{} requires a mode. TODO USAGE", prog_name);
        original_terminal.restore();
        return Ok(());
    }

    assert!(submode != Submode::NONE);

    // BTreeMap:  line => (index, score)
    let mut content_map: BTreeMap<(usize, &str),usize> = BTreeMap::new();
    let mut visited: HashMap<&str,u8> = HashMap::new(); // this is used in STDIN mode

    let mut content_bytes = Vec::new();
    let content;

    /* HISTORY and FILE modes both read files */
    if mode == Mode::HISTORY || mode == Mode::FILE {

        // HISTORY : If not file was provided, try reading $HISTFILE environment var
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
                return Err(e)
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

                // if no duplicate do this
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
            //println!("{}{}", stream, boxed);
        }
        
    /* STDIN mode */
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

    // overwrite previous output if in COMPILE mode
    // TODO: but do this only if output was displayed in stdout
    if mode == Mode::COMPILE {
        //erase_current_output(content_map.len());
        //io::stdout().flush().unwrap();
    }

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
    
    // do this when we are in Mode::STDIN because grabbing key press cannot
    // be done on the same STDIN we already read from...
    // https://users.rust-lang.org/t/how-to-read-user-input-again-in-pipeline/50576
    let mut stdin = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
    let fd = stdin.as_raw_fd();
    
    if mode == Mode::STDIN {
        original_terminal.fd = fd;
        original_terminal.save();
        rush_terminal.fd = fd;
        rush_terminal.save();
        rush_terminal.enable_raw_mode();
    }
    
    if content_map.len() == 0 {
        // nothing to do
        original_terminal.restore();
        return Ok(());
    }
    
    /* immediately print user prompt  and compute first iteration */
    if mode != Mode::COMPILE {
        print_user_search(&mut user_search);
        let _ = compute_scores(&mut content_map, &mut user_search.bytes, &mode);
    }

    let mut selected;
    if mode != Mode::COMPILE {
        selected = display_lines(&content_map,0, 0, 0, MAX_ROWS, &rush_terminal, &mode);
    } else {
        selected = display_lines_compile(&content_map,line_start_index, &mut line_highlight_cursor, MAX_ROWS, &rush_terminal);
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
                //print!("key = <{}> \n", key);
                //io::stdout().flush().unwrap();
                
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
                            rush_terminal.hide_cursor();
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
                        rush_terminal.clear();
                        rush_terminal.update_term_size();
                        MAX_ROWS = rush_terminal.height as usize - 1;
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
                            /* poor emulation of emacs compilation mode */
                            let mut output: Vec<(String,usize)> = Vec::new();
                            cmd_capture_output(&cmd_string, &mut output);

                            content_map.clear();

                            let mut index = 0;
                            for (line,stream) in output {
                                index += 1;
                                let boxed = Box::leak(line.into_boxed_str());
                                content_map.insert((index,boxed),stream as usize);
                            }
                            //cmd_capture_output(&cmd_string, &mut content_map);
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
                rush_terminal.update_term_size();
                erase_current_output(displayed_count);

                if mode != Mode::COMPILE {
                    print_user_search(&mut user_search);
                    selected = display_lines(&content_map,1, line_start_index, line_highlight_cursor, MAX_ROWS, &rush_terminal, &mode);
                } else {
                    selected = display_lines_compile(&content_map,line_start_index, &mut line_highlight_cursor, MAX_ROWS, &rush_terminal);
                }
                
                /* recompute displayed */
                displayed_count = if showable_lines > MAX_ROWS {MAX_ROWS} else {showable_lines};// - line_start_index;
            },

        }
        io::stdout().flush().unwrap();
    }
    
    erase_current_output(displayed_count);
    original_terminal.restore();

    ////////////////////////////////////////////////////////////
    // trying to print in CLI input buffer the selected command

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
