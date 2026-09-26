use std::ffi::{c_int,c_uint,c_uchar,c_ulong,c_ushort};
use std::fs::OpenOptions;
use std::os::fd::AsRawFd;

// For unsafe section with C code we need to ensure proper initialization of
// variables, especially when we want to optimize our code
// https://doc.rust-lang.org/std/mem/union.MaybeUninit.html
use std::mem::MaybeUninit; // used below to avoid trouble when compiler optimization

// C types for raw terminal handling
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

#[derive(Default)]
struct Winsize {
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

pub const STREAM_STDIN: c_int = 0;

/* terminal attributes */
const TCSAFLUSH: c_int = 2;

/* terminal flags */
const ECHO:   tcflag_t = 0b1000;
const ICANON: tcflag_t = 0b100;

/* windows size */
const TIOCGWINSZ: c_ulong = 0x5413;

pub struct Rush_Term {
    termios: Termios,
    pub fd: i32,
    pub height: u16, // available rows
    pub width: u16, // available cols
}

impl Rush_Term {

    pub fn new() -> Self {
        Self {
            termios: Default::default(),
            fd: STREAM_STDIN,
            height: 0,
            width: 0,
        }
    }

    pub fn erase_up(&self, rows: usize) {
        if rows != 0 {
            print!("\x1B[{}K", rows); // erase matching cmd rows + user input
            print!("\x1B[{}A\x1B[0J", rows); // move cursor up and clean up lines
        }
        print!("\x1B[{}G", 0); // cursor to bol
        print!("\x1B[0K\r");
    }

    pub fn clear(&self) {
        print!("\x2B[2J\x1B[1;1H");
    }

    pub fn hide_cursor(&self) {
        print!("\x1b[?25l");
    }

    fn show_cursor(&self) {
        print!("\x1b[?25h");
    }

    pub fn save(&mut self) -> () {
        unsafe { tcgetattr(self.fd, &mut self.termios); }
    }

    pub fn restore(&mut self) -> () {
        unsafe { tcsetattr(self.fd, TCSAFLUSH, &mut self.termios); }
        self.show_cursor();
    }

    pub fn enable_raw_mode(&mut self) -> () {
        self.termios.c_lflag &= ICANON; // enable ICANON  raw mode
        self.termios.c_lflag &= ! ECHO ; // disable ECHO mode
        unsafe { tcsetattr(self.fd, TCSAFLUSH, &mut self.termios); }
        self.hide_cursor();
    }

    pub fn update_term_size(&mut self) {
        //unsafe {
        //    let w: winsize = std::mem::zeroed();
        //    ioctl(STDIN,TIOCGWINSZ,&w);
        //    //println!("w.ws_row = {:?} ; w.ws_col = {:?}", w.ws_row, w.ws_col);
        //    *rows = w.ws_row as u16;
        //    *cols = w.ws_col as u16;
        //}

        // this will also work in STDIN mode
        let tty = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty").unwrap();

        unsafe {
            // https://doc.rust-lang.org/std/mem/union.MaybeUninit.html
            let mut w = MaybeUninit::<Winsize>::zeroed();       // <-- avoid trouble if compiler optimization
            ioctl(tty.as_raw_fd(), TIOCGWINSZ, w.as_mut_ptr()); // <-- same for `w.as_mut_ptr()`
            let w = w.assume_init();                            // <-- same
            self.height = w.ws_row as u16;
            self.width  = w.ws_col as u16;
            //println!("height === {}", self.height);
            //println!("width === {}", self.width);
        };
        
    }
}
