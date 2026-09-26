use {BTreeMap, Path, Command, Stdio};
use std::io::{BufRead,BufReader};
use env;
use channel;
use thread;

use {Rush_Term, Mode};
use {PROMPT_TEXT, MAX_SEARCH_LEN};

pub struct Location {
    pub file_path: String,
    pub row: u64,
    pub col: u64,
}

impl Location {
    pub fn new() -> Self {
        Self {
            file_path: "".to_string(),
            row: 0,
            col: 0,
        }
    }
}

/* Colors */
pub const default_color       : &str = "\x1b[0;37;49m";
pub const highlight_color     : &str = "\x1b[1;37;48;5;237m";
pub const default_headblock   : &str = "\x1b[0;32;48;5;237m";
pub const highlight_headblock : &str = "\x1b[0;;42m";

// for COMPILE colors
pub const default_err_color       : &str = "\x1b[0;31;48;5;235m";
pub const highlight_err_color     : &str = "\x1b[1;31;48;5;237m";
pub const default_err_headblock   : &str = "\x1b[0;32;48;5;1m";
pub const highlight_err_headblock : &str = "\x1b[0;;41m";

pub const default_empty_line_color       : &str = "\x1b[0;30;48;5;235m";
pub const highlight_empty_line_color     : &str = "\x1b[1;30;48;5;237m";
pub const default_empty_line_headblock   : &str = "\x1b[1;32;48;5;241m";
pub const highlight_empty_line_headblock : &str = "\x1b[1;32;48;5;241m";

pub fn compute_scores<'a>(content: &mut BTreeMap<(usize, &'a str),usize>, user_search: &mut [u8;MAX_SEARCH_LEN], _mode: &Mode)  -> Result<(), Box<dyn std::error::Error>> {
    
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

pub fn display_lines_default<'a>(content: &'a BTreeMap<(usize, &'a str),usize>, min_score: usize, start_index: usize, highlight_cursor: usize, max_rows: usize, term: &Rush_Term, mode: &Mode) -> (usize, &'a str) {
    
    assert!(*mode != Mode::COMPILE);
    
    /* display cmds */
    let mut displayed: usize = 0;
    let mut selected: (usize, &'a str) = (0,""); // selected key from content_cmds
    
    let pad = PROMPT_TEXT.len() + 10;
    let c = term.width as usize - pad;

    let mut index = 0;

    for (key,_score) in content.iter().filter(|((_,_),s)| **s >= min_score) {
        
        if index < start_index {
            index += 1;
            continue;
        }

        let (_, cmd) = *key;
        
        if displayed >= max_rows { break }

        print!("\n{}", " ".repeat(PROMPT_TEXT.len()));
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

pub fn display_lines_compile<'a>(content: &'a BTreeMap<(usize, &'a str),usize>, start_index: usize, highlight_cursor: &mut usize, max_rows: usize, term: &Rush_Term) -> (usize, &'a str) {

    //assert!(*mode == Mode::COMPILE);
    
    /* display cmds */
    let mut displayed: usize = 0;
    let mut selected: (usize, &'a str) = (0,""); // selected key from content_cmds
    
    let pad = PROMPT_TEXT.len() + 10;
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

        print!("\n{}", " ".repeat(PROMPT_TEXT.len()));
        
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

pub fn get_location_from_line(line: &str) -> Option<Location> {
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
pub fn cmd_capture_output<'a>(cmd_string: &String, content: &mut Vec<(String, usize)>) {
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

pub fn open_in_editor(selected_line: (usize,&str), content_file_path: String, mode: &Mode) {
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
